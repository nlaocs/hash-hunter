use crate::hash::{FocusDigits, digit};

const CANDIDATE_LIMIT: usize = 24;
const MIN_REPEAT_BITS: f64 = 7.0;

pub const WORDS: &[&str] = &[
    "deadbeef", "feedface", "cafebabe", "f00dbabe", "b16b00b5", "f00dface", "decafbad", "facefeed",
    "deadc0de", "badc0de", "defaced", "0ddba11", "c0ffee", "bada55", "decaf", "f00d", "cafe",
    "beef", "face", "feed", "babe", "dead", "1337", "ba5e", "abba", "bead",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Run,
    Repeat(usize),
    Palindrome,
    Sequence { ascending: bool },
    Word,
    Border,
}

#[derive(Clone, Copy, Debug)]
pub struct Pattern {
    pub kind: Kind,
    pub start: usize,
    pub len: usize,
    fixed_bits: f64,
    pub starts: u64,
    pub run_domain: u16,
}

impl Pattern {
    pub fn new(kind: Kind, start: usize, len: usize, hash_len: usize, fixed_bits: f64) -> Self {
        let last = hash_len - len;
        let edges = 1 | (1 << last);
        let starts = match kind {
            Kind::Border => 1,
            Kind::Run if start == 0 || start == last => edges,
            Kind::Run => position_mask(0, last + 1) & !edges,
            _ => position_mask(0, last + 1),
        };
        Self {
            kind,
            start,
            len,
            fixed_bits,
            starts,
            run_domain: u16::MAX,
        }
    }

    pub fn end(self) -> usize {
        self.start + self.len
    }

    pub fn coverage(self, hash_len: usize) -> u64 {
        let first = position_mask(self.start, self.len);
        if self.kind == Kind::Border {
            first | position_mask(hash_len - self.len, self.len)
        } else {
            first
        }
    }

    pub fn fixed_bits(self) -> f64 {
        if self.run_domain == u16::MAX {
            self.fixed_bits
        } else {
            self.fixed_bits + 4.0 - f64::from(self.run_domain.count_ones()).log2()
        }
    }

    pub fn bits(self) -> f64 {
        self.fixed_bits() - f64::from(self.starts.count_ones()).log2()
    }

    fn unfocused_bits(self) -> f64 {
        self.fixed_bits - f64::from(self.starts.count_ones()).log2()
    }

    pub fn label(self, text: &str, show_focus: bool) -> String {
        let prefix = match self.kind {
            Kind::Run if show_focus && self.run_domain != u16::MAX => "指定文字のゾロ目",
            Kind::Run => "ゾロ目",
            Kind::Repeat(period) => {
                return format!("周期{period}:{}", &text[self.start..self.end()]);
            }
            Kind::Palindrome => "回文",
            Kind::Sequence { ascending: true } => "昇順",
            Kind::Sequence { ascending: false } => "降順",
            Kind::Word => "ワード",
            Kind::Border => "先頭=末尾",
        };
        format!("{prefix}:{}", &text[self.start..self.end()])
    }

    pub fn contains(self, child: Self) -> bool {
        self.kind != Kind::Border
            && child.kind != Kind::Border
            && self.start <= child.start
            && child.end() <= self.end()
    }

    pub fn centers_run(self, child: Self) -> bool {
        self.kind == Kind::Palindrome
            && child.kind == Kind::Run
            && self.contains(child)
            && self.start + self.end() == child.start + child.end()
    }

    pub fn center_only(self) -> bool {
        self.kind == Kind::Run && self.len == 3
    }

    pub fn same_family(self, other: Self) -> bool {
        let same_kind = self.kind == other.kind
            || matches!(
                (self.kind, other.kind),
                (Kind::Sequence { .. }, Kind::Sequence { .. })
            );
        same_kind
            && self.len == other.len
            && self.starts == other.starts
            && self.run_domain == other.run_domain
    }
}

pub fn position_mask(start: usize, len: usize) -> u64 {
    (u64::MAX >> (64 - len)) << start
}

pub struct Detector {
    focus: FocusDigits,
    word_heads: Box<[u32]>,
    word_counts: [u32; 9],
}

impl Detector {
    pub fn new(focus: FocusDigits) -> Self {
        let mut word_heads = vec![0; 1 << 16].into_boxed_slice();
        let mut word_counts = [0; 9];
        for (index, word) in WORDS.iter().enumerate() {
            let head = word.bytes().take(4).fold(0u16, |head, byte| {
                (head << 4) | u16::from(digit(byte).expect("hex dictionary"))
            });
            word_heads[usize::from(head)] |= 1u32 << index;
            word_counts[word.len()] += 1;
        }
        Self {
            focus,
            word_heads,
            word_counts,
        }
    }

    pub fn detect(&self, digits: &[u8], patterns: &mut Vec<Pattern>) {
        patterns.clear();
        self.runs_and_repeats(digits, patterns);
        palindromes(digits, patterns, self.focus);
        sequences(digits, patterns);
        self.words(digits, patterns);
        if let Some(pattern) = border(digits) {
            patterns.push(pattern);
        }
        if patterns.len() > CANDIDATE_LIMIT {
            // The focus option must not change the available visual candidates.
            patterns.sort_by(|a, b| b.unfocused_bits().total_cmp(&a.unfocused_bits()));
            patterns.truncate(CANDIDATE_LIMIT);
        }
    }

    fn runs_and_repeats(&self, digits: &[u8], patterns: &mut Vec<Pattern>) {
        for period in 1..=8 {
            let mut equal = equal_positions(digits, period);
            // A run needs three equalities, and shorter period-2 repeats
            // fall below the rarity threshold for either hash length.
            if equal & (equal >> 1) & (equal >> 2) == 0 {
                continue;
            }
            while equal != 0 {
                let start = equal.trailing_zeros() as usize;
                let matches = (equal >> start).trailing_ones() as usize;
                equal &= u64::MAX << (start + matches);

                let len = matches + period;
                let positions = (digits.len() - len + 1) as u32;
                let fixed_bits = 4.0 * matches as f64;
                if period == 1 && len >= 4 {
                    patterns.push(run(digits, start, len, self.focus));
                } else if period > 1
                    && matches >= period
                    && fixed_bits - f64::from(positions).log2() >= MIN_REPEAT_BITS
                {
                    patterns.push(Pattern::new(
                        Kind::Repeat(period),
                        start,
                        len,
                        digits.len(),
                        fixed_bits,
                    ));
                }
            }
        }
    }

    fn words(&self, digits: &[u8], patterns: &mut Vec<Pattern>) {
        let mut head = digits[..4]
            .iter()
            .fold(0u16, |head, &d| (head << 4) | u16::from(d));
        for start in 0..=digits.len() - 4 {
            let mut candidates = self.word_heads[usize::from(head)];
            while candidates != 0 {
                let index = candidates.trailing_zeros() as usize;
                candidates &= candidates - 1;
                let word = WORDS[index];
                let len = word.len();
                if let Some(tail) = digits.get(start + 4..start + len)
                    && tail.iter().copied().eq(word
                        .bytes()
                        .skip(4)
                        .map(|b| digit(b).expect("hex dictionary")))
                {
                    let fixed_bits = 4.0 * len as f64 - f64::from(self.word_counts[len]).log2();
                    patterns.push(Pattern::new(
                        Kind::Word,
                        start,
                        len,
                        digits.len(),
                        fixed_bits,
                    ));
                }
            }
            if let Some(&next) = digits.get(start + 4) {
                head = (head << 4) | u16::from(next);
            }
        }
    }
}

fn run(digits: &[u8], start: usize, len: usize, focus: FocusDigits) -> Pattern {
    let mut pattern = Pattern::new(Kind::Run, start, len, digits.len(), 4.0 * (len - 1) as f64);
    if focus.contains(digits[start]) {
        pattern.run_domain = focus.mask();
    }
    pattern
}

fn word4(digits: &[u8]) -> u32 {
    u32::from_le_bytes(digits[..4].try_into().expect("four digits"))
}

fn palindromes(digits: &[u8], patterns: &mut Vec<Pattern>, focus: FocusDigits) {
    for middle in [1, 0] {
        for center in 4..=digits.len() - 4 - middle {
            let mut start = center - 4;
            let mut end = center + middle + 4;
            if word4(&digits[start..]) != word4(&digits[center + middle..]).swap_bytes() {
                continue;
            }
            while start > 0 && end < digits.len() && digits[start - 1] == digits[end] {
                start -= 1;
                end += 1;
            }
            let len = end - start;
            patterns.push(Pattern::new(
                Kind::Palindrome,
                start,
                len,
                digits.len(),
                4.0 * (len / 2) as f64,
            ));
            // The palindrome already establishes equality with the right-hand side.
            if middle == 1
                && digits[center - 1] == digits[center]
                && digits[center - 2] != digits[center]
            {
                patterns.push(run(digits, center - 1, 3, focus));
            }
        }
    }
}

fn sequences(digits: &[u8], patterns: &mut Vec<Pattern>) {
    for ascending in [true, false] {
        let mut start = 0;
        for end in 1..=digits.len() {
            if let Some(&next) = digits.get(end) {
                let previous = digits[end - 1];
                let extends = if ascending {
                    next == previous + 1
                } else {
                    previous == next + 1
                };
                if extends {
                    continue;
                }
            }
            let len = end - start;
            if len >= 6 {
                let fixed_bits = 4.0 * len as f64 - ((2 * (17 - len)) as f64).log2();
                patterns.push(Pattern::new(
                    Kind::Sequence { ascending },
                    start,
                    len,
                    digits.len(),
                    fixed_bits,
                ));
            }
            start = end;
        }
    }
}

fn border(digits: &[u8]) -> Option<Pattern> {
    let prefix = word4(digits);
    (4..=digits.len() / 2).rev().find_map(|len| {
        let suffix = &digits[digits.len() - len..];
        (word4(suffix) == prefix && digits[..len] == *suffix)
            .then(|| Pattern::new(Kind::Border, 0, len, digits.len(), 4.0 * len as f64))
    })
}

fn equal_positions(digits: &[u8], distance: usize) -> u64 {
    let (left, left_tail) = digits[..digits.len() - distance].as_chunks::<8>();
    let (right, right_tail) = digits[distance..].as_chunks::<8>();
    let mut positions = 0;
    for (index, (&a, &b)) in left.iter().zip(right).enumerate() {
        let a = u64::from_le_bytes(a);
        let b = u64::from_le_bytes(b);
        // Nibbles have no high bits. Adding 0x7f per byte sets the high bit
        // exactly where the XOR is nonzero, without a carry between bytes.
        let equal_bytes = !((a ^ b) + 0x7f7f_7f7f_7f7f_7f7f) & 0x8080_8080_8080_8080;
        // Pack the high bit of each byte into consecutive bits.
        let bits = equal_bytes.wrapping_mul(0x0002_0408_1020_4081) >> 56;
        positions |= bits << (index * 8);
    }
    let tail = left_tail.iter().zip(right_tail);
    let start = left.len() * 8;
    for (index, (a, b)) in tail.enumerate() {
        positions |= u64::from(a == b) << (start + index);
    }
    positions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equality_masks_match_scalar_comparisons() {
        let mut seed = 12345u64;
        for len in [40, 64] {
            for _ in 0..256 {
                let digits: Vec<_> = (0..len)
                    .map(|_| {
                        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                        (seed >> 60) as u8
                    })
                    .collect();
                for distance in 1..=8 {
                    let expected = digits
                        .iter()
                        .zip(&digits[distance..])
                        .enumerate()
                        .map(|(i, (a, b))| u64::from(a == b) << i)
                        .sum();
                    assert_eq!(equal_positions(&digits, distance), expected);
                }
            }
            for distance in 1..=8 {
                assert_eq!(
                    equal_positions(&vec![15; len], distance),
                    (1u64 << (len - distance)) - 1
                );
            }
        }
    }
}
