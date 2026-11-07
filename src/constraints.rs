use crate::hash::digit;
use crate::pattern::{Kind, Pattern, WORDS, position_mask};

pub fn disjoint_placements(hash_len: usize, mut left: Pattern, mut right: Pattern) -> u32 {
    if right.kind == Kind::Border {
        std::mem::swap(&mut left, &mut right);
    }
    debug_assert_ne!(right.kind, Kind::Border);

    let overlapping_starts = |start: usize| {
        let first = start.saturating_sub(right.len - 1);
        position_mask(first, start + left.len - first)
    };
    let mut starts = left.starts;
    let mut count = 0;
    while starts != 0 {
        let start = starts.trailing_zeros() as usize;
        starts &= starts - 1;
        let mut forbidden = overlapping_starts(start);
        if left.kind == Kind::Border {
            forbidden |= overlapping_starts(hash_len - left.len);
        }
        count += (right.starts & !forbidden).count_ones();
    }
    if left.same_family(right) {
        count / 2
    } else {
        count
    }
}

pub fn translations(patterns: &[Pattern]) -> u32 {
    let first = patterns.iter().map(|pattern| pattern.start).min().unwrap();
    patterns
        .iter()
        .fold(u64::MAX, |positions, pattern| {
            positions & (pattern.starts >> (pattern.start - first))
        })
        .count_ones()
}

pub fn joint_bits(hash_len: usize, patterns: &[Pattern]) -> f64 {
    let mut sets = Components::new();
    for &pattern in patterns {
        let start = pattern.start;
        let end = pattern.end();
        match pattern.kind {
            Kind::Run => {
                for position in start + 1..end {
                    sets.join(start, position);
                }
            }
            Kind::Repeat(period) => {
                for position in start + period..end {
                    sets.join(position - period, position);
                }
            }
            Kind::Palindrome => {
                for offset in 0..pattern.len / 2 {
                    sets.join(start + offset, end - 1 - offset);
                }
            }
            Kind::Border => {
                // A border links two blocks, with the second ending at the hash boundary.
                let second = hash_len - pattern.len;
                for offset in 0..pattern.len {
                    sets.join(offset, second + offset);
                }
            }
            Kind::Word | Kind::Sequence { .. } => {}
        }
    }

    let roots = std::array::from_fn(|i| sets.root(i));
    let mut domains = [u16::MAX; 64];
    for pattern in patterns {
        if pattern.kind == Kind::Run {
            domains[usize::from(roots[pattern.start])] &= pattern.run_domain;
        }
    }

    // At this placement, distinct assignments for any word or sequence
    // are disjoint events. Sum the compatible assignments' probabilities.
    let fraction = compatible_fraction(patterns, &roots, &domains);
    debug_assert!(
        fraction > 0.0,
        "the detected layout has at least one compatible assignment"
    );
    4.0 * f64::from(sets.joins) - fraction.log2()
}

fn compatible_fraction(patterns: &[Pattern], roots: &[u8; 64], domains: &[u16; 64]) -> f64 {
    let Some((&pattern, rest)) = patterns.split_first() else {
        return domains
            .iter()
            .filter(|&&domain| domain != u16::MAX)
            .map(|domain| f64::from(domain.count_ones()) / 16.0)
            .product();
    };
    match pattern.kind {
        Kind::Word => WORDS
            .iter()
            .filter(|word| word.len() == pattern.len)
            .map(|word| {
                let values = word
                    .bytes()
                    .map(|byte| digit(byte).expect("hex dictionary"));
                literal_fraction(pattern.start, values, rest, roots, *domains)
            })
            .sum(),
        Kind::Sequence { .. } => {
            let mut fraction = 0.0;
            for ascending in [true, false] {
                for low in 0..=16 - pattern.len {
                    let values = (0..pattern.len).map(|offset| {
                        (low + if ascending {
                            offset
                        } else {
                            pattern.len - 1 - offset
                        }) as u8
                    });
                    fraction += literal_fraction(pattern.start, values, rest, roots, *domains);
                }
            }
            fraction
        }
        _ => compatible_fraction(rest, roots, domains),
    }
}

fn literal_fraction(
    start: usize,
    values: impl Iterator<Item = u8>,
    rest: &[Pattern],
    roots: &[u8; 64],
    mut domains: [u16; 64],
) -> f64 {
    for (offset, digit) in values.enumerate() {
        let domain = &mut domains[usize::from(roots[start + offset])];
        *domain &= 1 << digit;
        if *domain == 0 {
            return 0.0;
        }
    }
    compatible_fraction(rest, roots, &domains)
}

struct Components {
    parent: [u8; 64],
    size: [u8; 64],
    joins: u8,
}

impl Components {
    fn new() -> Self {
        Self {
            parent: std::array::from_fn(|i| i as u8),
            size: [1; 64],
            joins: 0,
        }
    }

    fn root(&mut self, position: usize) -> u8 {
        let parent = self.parent[position];
        if usize::from(parent) == position {
            return parent;
        }
        let root = self.root(usize::from(parent));
        self.parent[position] = root;
        root
    }

    fn join(&mut self, left: usize, right: usize) {
        let mut left = usize::from(self.root(left));
        let mut right = usize::from(self.root(right));
        if left == right {
            return;
        }
        if self.size[left] < self.size[right] {
            std::mem::swap(&mut left, &mut right);
        }
        self.parent[right] = left as u8;
        self.size[left] += self.size[right];
        self.joins += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern(kind: Kind, start: usize, len: usize) -> Pattern {
        Pattern::new(kind, start, len, 40, 0.0)
    }

    #[test]
    fn overlapping_equalities_are_counted_once() {
        let palindrome = pattern(Kind::Palindrome, 0, 12);
        let center = pattern(Kind::Run, 4, 4);
        let left = pattern(Kind::Run, 2, 2);
        let right = pattern(Kind::Run, 8, 2);
        let mut focused = center;
        focused.run_domain = (1 << 0) | (1 << 5);

        assert_eq!(joint_bits(40, &[palindrome, center]), 28.0);
        assert_eq!(joint_bits(40, &[focused, palindrome]), 31.0);
        assert_eq!(joint_bits(40, &[palindrome, left, right]), 28.0);
    }

    #[test]
    fn overlapping_sequences_count_only_compatible_directions_and_values() {
        let first = pattern(Kind::Sequence { ascending: true }, 0, 6);
        let second = pattern(Kind::Sequence { ascending: true }, 1, 6);
        let bits = joint_bits(40, &[first, second]);
        assert!((bits - (28.0 - 20f64.log2())).abs() < 1e-12);
        assert_eq!(joint_bits(40, &[second, first]), bits);
    }

    #[test]
    fn overlapping_words_count_three_compatible_word_pairs() {
        let left = pattern(Kind::Word, 0, 8);
        let right = pattern(Kind::Word, 4, 8);
        assert_eq!(translations(&[left, right]), 29);
        assert!((joint_bits(40, &[left, right]) - (48.0 - 3f64.log2())).abs() < 1e-12);
    }

    #[test]
    fn separate_words_count_each_pair_once() {
        let left = pattern(Kind::Word, 0, 8);
        let right = pattern(Kind::Word, 10, 8);
        assert_eq!(disjoint_placements(40, left, right), 325);
        assert_eq!(disjoint_placements(40, right, left), 325);
    }

    #[test]
    fn border_anchors_overlap_and_uses_both_ends_for_disjoint_placements() {
        let border = pattern(Kind::Border, 0, 4);
        let word = pattern(Kind::Word, 36, 4);
        assert_eq!(translations(&[border, word]), 1);
        assert_eq!(disjoint_placements(40, border, word), 29);
        assert!((joint_bits(40, &[border, word]) - (32.0 - 11f64.log2())).abs() < 1e-12);
    }
}
