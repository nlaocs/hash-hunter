use std::fmt::Write as _;

use crate::{
    constraints::fixed_bits,
    hash::{FocusDigits, HEX, ObjectId},
    pattern::{Detector, Kind, Pattern},
};

#[derive(Clone, Copy, Debug)]
enum Relation {
    Single,
    General,
    Contained,
    Centered,
    Triple,
}

impl Relation {
    fn label(self) -> &'static str {
        match self {
            Self::Single => "単独",
            Self::General => "複合",
            Self::Contained => "入れ子の複合",
            Self::Centered => "中心対称の複合",
            Self::Triple => "三重複合",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LocalScore {
    ids: [usize; 3],
    count: usize,
    relation: Relation,
    bits: f64,
    pub fixed_bits: f64,
    pub opportunities: u32,
}

impl LocalScore {
    fn ids(&self) -> &[usize] {
        &self.ids[..self.count]
    }
}

#[derive(Clone, Copy, Debug)]
enum Winner {
    Local(LocalScore),
    Frequency { value: usize, count: u8, bits: f64 },
    AllDigits(f64),
    AllLetters(f64),
}

impl Winner {
    fn bits(self) -> f64 {
        match self {
            Self::Local(score) => score.bits,
            Self::Frequency { bits, .. } | Self::AllDigits(bits) | Self::AllLetters(bits) => bits,
        }
    }
}

pub struct Assessment<'a> {
    pub patterns: &'a [Pattern],
    winner: Winner,
    pub beauty: u8,
}

impl Assessment<'_> {
    pub fn bits(&self) -> f64 {
        self.winner.bits()
    }

    pub fn local_score(&self) -> Option<LocalScore> {
        match self.winner {
            Winner::Local(score) => Some(score),
            _ => None,
        }
    }

    pub fn composite(&self) -> bool {
        self.local_score().is_some_and(|score| score.count > 1)
    }

    pub fn primary(&self, text: &str) -> String {
        match self.winner {
            Winner::Local(score) => {
                let mut labels = score
                    .ids()
                    .iter()
                    .map(|&id| self.patterns[id].label(text))
                    .collect::<Vec<_>>()
                    .join(" + ");
                if score.count > 1 {
                    write!(labels, " [{}]", score.relation.label()).expect("write to String");
                }
                labels
            }
            Winner::Frequency { value, count, .. } => {
                format!("文字頻度:{}×{count}", char::from(HEX[value]))
            }
            Winner::AllDigits(_) => "数字のみ".into(),
            Winner::AllLetters(_) => "a-fのみ".into(),
        }
    }

    pub fn details(&self, text: &str) -> String {
        let mut labels = Vec::new();
        for &pattern in self.patterns {
            let label = pattern.label(text);
            if !labels.contains(&label) {
                labels.push(label);
            }
        }
        labels.join(" | ")
    }
}

pub struct Scorer {
    detector: Detector,
    frequency40: [f64; 65],
    frequency64: [f64; 65],
}

impl Scorer {
    pub fn new(focus: FocusDigits) -> Self {
        Self {
            detector: Detector::new(focus),
            frequency40: frequency_scores(40),
            frequency64: frequency_scores(64),
        }
    }

    pub fn analyze<'a>(
        &self,
        hash: &ObjectId,
        patterns: &'a mut Vec<Pattern>,
    ) -> Option<Assessment<'a>> {
        let digits = hash.digits();
        self.detector.detect(digits, patterns);
        let local = self.best_local(digits, patterns).map(Winner::Local);
        let global = self.best_global(digits);
        let winner = match (local, global) {
            (Some(local), Some(global)) if global.bits() > local.bits() => global,
            (Some(local), _) => local,
            (None, global) => global?,
        };
        Some(Assessment {
            patterns,
            winner,
            beauty: beauty(digits, patterns, winner),
        })
    }

    fn best_local(&self, digits: &[u8], patterns: &[Pattern]) -> Option<LocalScore> {
        let mut best: Option<LocalScore> = None;
        let mut offer = |ids: &[usize], relation, fixed_bits: f64, opportunities: u32| {
            let bits = fixed_bits - f64::from(opportunities).log2();
            if bits > 0.0 && best.is_none_or(|previous| bits > previous.bits + 1e-9) {
                let mut selected = [0; 3];
                selected[..ids.len()].copy_from_slice(ids);
                best = Some(LocalScore {
                    ids: selected,
                    count: ids.len(),
                    relation,
                    bits,
                    fixed_bits,
                    opportunities,
                });
            }
        };
        for (id, pattern) in patterns.iter().enumerate() {
            offer(
                &[id],
                Relation::Single,
                pattern.fixed_bits,
                pattern.opportunities,
            );
        }
        for (i, &a) in patterns.iter().enumerate() {
            for (j, &b) in patterns.iter().enumerate().skip(i + 1) {
                let (opportunities, relation) = self.pair_opportunities(a, b);
                offer(
                    &[i, j],
                    relation,
                    fixed_bits(digits, &[a, b]),
                    opportunities,
                );
            }
        }
        for (parent_id, &parent) in patterns.iter().enumerate() {
            if parent.kind != Kind::Palindrome {
                continue;
            }
            let children = patterns
                .iter()
                .copied()
                .enumerate()
                .filter(|&(id, pattern)| id != parent_id && parent.contains(pattern));
            for (i, a) in children.clone() {
                for (j, b) in children.clone().filter(|&(id, _)| id > i) {
                    if !a.kind.is_run() && !b.kind.is_run() {
                        continue;
                    }
                    if a.kind.is_run() && b.kind.is_run() && a.start == b.start && a.len == b.len {
                        continue;
                    }
                    let opportunities = parent.opportunities
                        * self.relative_opportunities(parent, a)
                        * self.relative_opportunities(parent, b);
                    offer(
                        &[parent_id, i, j],
                        Relation::Triple,
                        fixed_bits(digits, &[parent, a, b]),
                        opportunities,
                    );
                }
            }
        }
        best
    }

    fn pair_opportunities(&self, a: Pattern, b: Pattern) -> (u32, Relation) {
        let (parent, child) = if a.contains(b) {
            (a, b)
        } else if b.contains(a) {
            (b, a)
        } else {
            return (a.opportunities * b.opportunities, Relation::General);
        };
        let relation = if parent.centers_run(child) {
            Relation::Centered
        } else {
            Relation::Contained
        };
        (
            parent.opportunities * self.relative_opportunities(parent, child),
            relation,
        )
    }

    fn relative_opportunities(&self, parent: Pattern, child: Pattern) -> u32 {
        let positions = if parent.centers_run(child) {
            1
        } else {
            (parent.len - child.len + 1) as u32
        };
        positions * self.detector.choices(child)
    }

    fn best_global(&self, digits: &[u8]) -> Option<Winner> {
        let mut counts = [0u8; 16];
        for &digit in digits {
            counts[usize::from(digit)] += 1;
        }
        let mut value = 0;
        for index in 1..counts.len() {
            if counts[index] > counts[value] {
                value = index;
            }
        }
        let count = counts[value];
        let frequency = if digits.len() == 40 {
            &self.frequency40
        } else {
            &self.frequency64
        };
        let bits = frequency[usize::from(count)];
        let mut best = (bits >= 6.0).then_some(Winner::Frequency { value, count, bits });

        let letters = counts[10..]
            .iter()
            .map(|&count| usize::from(count))
            .sum::<usize>();
        let category = if letters == 0 {
            Some(Winner::AllDigits(
                digits.len() as f64 * (16.0f64 / 10.0).log2(),
            ))
        } else if letters == digits.len() {
            Some(Winner::AllLetters(
                digits.len() as f64 * (16.0f64 / 6.0).log2(),
            ))
        } else {
            None
        };
        if let Some(category) = category {
            if best.is_none_or(|previous| category.bits() > previous.bits()) {
                best = Some(category);
            }
        }
        best
    }
}

fn frequency_scores(len: usize) -> [f64; 65] {
    let mut probabilities = [0.0; 65];
    probabilities[0] = (15.0f64 / 16.0).powi(len as i32);
    for count in 1..=len {
        probabilities[count] =
            probabilities[count - 1] * (len - count + 1) as f64 / (15.0 * count as f64);
    }
    let mut bits = [0.0; 65];
    let mut tail = 0.0;
    for count in (0..=len).rev() {
        tail += probabilities[count];
        bits[count] = -(16.0 * tail).min(1.0).log2();
    }
    bits
}

fn beauty(digits: &[u8], patterns: &[Pattern], winner: Winner) -> u8 {
    let mut points = patterns
        .iter()
        .map(|pattern| {
            let len = pattern.len;
            match pattern.kind {
                Kind::Run => (5 * len + 9 * usize::from(digits[pattern.start] == 0)).min(68),
                Kind::FocusedRun => (5 * len + 12).min(75),
                Kind::Repeat(period) => (4 * len + 3 * (len / period)).min(72),
                Kind::Palindrome => (4 * len).min(70),
                Kind::Sequence { .. } => (6 * len).min(68),
                Kind::Word => (7 * len + 10).min(75),
                Kind::Border => (8 * len).min(75),
            }
        })
        .max()
        .unwrap_or(0);
    if let Winner::Local(local) = winner {
        points += match local.relation {
            Relation::Single => 0,
            Relation::General => 16,
            Relation::Contained => 23,
            Relation::Centered => 37,
            Relation::Triple => 32,
        };
        for &parent in local.ids() {
            for &child in local.ids() {
                let run = patterns[child];
                if patterns[parent].centers_run(run) && digits[run.start] == 0 {
                    points += 9;
                }
            }
        }
    }
    points.min(100) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash_with(text: &str, offset: usize) -> ObjectId {
        let mut base = String::from("d84a73bc621f95e0a347c125d89b6e04f13a2cb8");
        base.replace_range(offset..offset + text.len(), text);
        base.parse().unwrap()
    }

    #[test]
    fn centered_run_and_focused_run() {
        let hash = hash_with("325500005523", 12);
        let mut patterns = Vec::new();
        for (focus, fixed, beauty) in [("", 28.0, 94), ("0", 32.0, 94)] {
            let scorer = Scorer::new(focus.parse().unwrap());
            let result = scorer.analyze(&hash, &mut patterns).unwrap();
            assert!((result.bits() - (fixed - 29f64.log2())).abs() < 1e-9);
            assert_eq!(result.beauty, beauty);
            assert!(result.primary(&hash.to_string()).contains("中心対称"));
            assert!(result.composite());
        }
    }

    #[test]
    fn edge_runs_score_by_length() {
        let scorer = Scorer::new("".parse().unwrap());
        let mut patterns = Vec::new();
        for (text, start, expected) in [
            ("fffff", 35, 15.0),
            ("a33333", 34, 15.0),
            ("333333", 34, 19.0),
        ] {
            let hash = hash_with(text, start);
            assert_eq!(
                scorer.analyze(&hash, &mut patterns).unwrap().bits(),
                expected
            );
        }
    }

    #[test]
    fn period_seven_is_detected() {
        let hash: ObjectId = "f355189036f38ad10c31ad10c31ff01bff34e0bf".parse().unwrap();
        let scorer = Scorer::new("".parse().unwrap());
        let mut patterns = Vec::new();
        let result = scorer.analyze(&hash, &mut patterns).unwrap();
        assert!(
            result
                .details(&hash.to_string())
                .contains("周期7:ad10c31ad10c31")
        );
    }
}
