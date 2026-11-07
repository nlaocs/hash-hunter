use crate::pattern::{Kind, Pattern};

pub fn fixed_bits(digits: &[u8], patterns: &[Pattern]) -> f64 {
    let mut used = 0;
    let mut disjoint = true;
    for pattern in patterns {
        let span = u64::MAX >> (64 - pattern.len);
        let occupied = if pattern.kind == Kind::Border {
            span | (span << (digits.len() - pattern.len))
        } else {
            span << pattern.start
        };
        disjoint &= used & occupied == 0;
        used |= occupied;
    }
    if disjoint {
        return patterns.iter().map(|pattern| pattern.fixed_bits).sum();
    }

    // Subtract each observed digit modulo 16. Every detected equality or
    // difference then relates two equal offsets, and fixed digits allow 0.
    let mut sets = Components::new();
    for &pattern in patterns {
        let start = pattern.start;
        let end = pattern.end();
        match pattern.kind {
            Kind::Run | Kind::FocusedRun | Kind::Sequence { .. } => {
                for position in start + 1..end {
                    sets.join(start, position);
                }
                match pattern.kind {
                    Kind::FocusedRun => sets.restrict(start, 1),
                    Kind::Sequence { ascending } => {
                        let low = u16::MAX >> (pattern.len - 1);
                        let allowed = if ascending {
                            low
                        } else {
                            low << (pattern.len - 1)
                        };
                        sets.restrict(start, allowed.rotate_right(u32::from(digits[start])));
                    }
                    _ => {}
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
            Kind::Word => {
                for position in start..end {
                    sets.restrict(position, 1);
                }
            }
            Kind::Border => {
                for offset in 0..pattern.len {
                    sets.join(offset, digits.len() - pattern.len + offset);
                }
            }
        }
    }
    sets.bits(digits.len())
}

struct Components {
    parent: [u8; 64],
    size: [u8; 64],
    domain: [u16; 64],
    joins: u8,
}

impl Components {
    fn new() -> Self {
        Self {
            parent: std::array::from_fn(|i| i as u8),
            size: [1; 64],
            domain: [u16::MAX; 64],
            joins: 0,
        }
    }

    fn root(&mut self, position: usize) -> usize {
        let parent = usize::from(self.parent[position]);
        if parent == position {
            return position;
        }
        let root = self.root(parent);
        self.parent[position] = root as u8;
        root
    }

    fn join(&mut self, left: usize, right: usize) {
        let mut left = self.root(left);
        let mut right = self.root(right);
        if left == right {
            return;
        }
        if self.size[left] < self.size[right] {
            std::mem::swap(&mut left, &mut right);
        }
        self.parent[right] = left as u8;
        self.size[left] += self.size[right];
        self.domain[left] &= self.domain[right];
        self.joins += 1;
    }

    fn restrict(&mut self, position: usize, allowed: u16) {
        let root = self.root(position);
        self.domain[root] &= allowed;
    }

    fn bits(&self, len: usize) -> f64 {
        let mut bits = 4.0 * f64::from(self.joins);
        for i in 0..len {
            let domain = self.domain[i];
            if usize::from(self.parent[i]) == i && domain != u16::MAX {
                debug_assert!(domain & 1 != 0, "the observed digits satisfy the patterns");
                bits += 4.0 - f64::from(domain.count_ones()).log2();
            }
        }
        bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::digit;

    fn pattern(kind: Kind, start: usize, len: usize) -> Pattern {
        Pattern {
            kind,
            start,
            len,
            fixed_bits: 0.0,
            opportunities: 1,
        }
    }

    #[test]
    fn overlapping_equalities_are_counted_once() {
        let digits: Vec<_> = b"325500005523"
            .iter()
            .map(|&byte| digit(byte).unwrap())
            .collect();
        let palindrome = pattern(Kind::Palindrome, 0, 12);
        let center = pattern(Kind::Run, 4, 4);
        let focused = pattern(Kind::FocusedRun, 4, 4);
        let left = pattern(Kind::Run, 2, 2);
        let right = pattern(Kind::Run, 8, 2);

        assert_eq!(fixed_bits(&digits, &[palindrome, center]), 28.0);
        assert_eq!(fixed_bits(&digits, &[focused, palindrome]), 32.0);
        assert_eq!(fixed_bits(&digits, &[palindrome, left, right]), 28.0);
    }

    #[test]
    fn overlapping_sequences_intersect_non_wrapping_domains() {
        let digits = [4, 5, 6, 7, 8, 9, 10];
        let ascending = Kind::Sequence { ascending: true };
        let first = pattern(ascending, 0, 6);
        let second = pattern(ascending, 1, 6);
        let bits = fixed_bits(&digits, &[first, second]);
        assert!((bits - (28.0 - 10f64.log2())).abs() < 1e-12);

        let digits = [10, 9, 8, 7, 6, 5, 4];
        let descending = Kind::Sequence { ascending: false };
        let first = pattern(descending, 0, 6);
        let second = pattern(descending, 1, 6);
        assert!((fixed_bits(&digits, &[second, first]) - bits).abs() < 1e-12);
    }

    #[test]
    fn border_uses_both_ends_when_testing_for_overlap() {
        let digits = [1, 2, 3, 4, 1, 2, 3, 4];
        let border = pattern(Kind::Border, 0, 4);
        let word = pattern(Kind::Word, 4, 4);
        assert_eq!(fixed_bits(&digits, &[border, word]), 32.0);
    }
}
