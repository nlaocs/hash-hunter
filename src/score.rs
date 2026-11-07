use std::fmt::Write as _;

use crate::{
    constraints::{disjoint_placements, joint_bits, translations},
    hash::{FocusDigits, HEX, ObjectId},
    pattern::{Detector, Kind, Pattern},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Relation {
    Single,
    Separate,
    Overlap,
    Contained,
    Centered,
    Triple,
}

impl Relation {
    fn between(left: Pattern, right: Pattern, hash_len: usize) -> Self {
        if left.coverage(hash_len) & right.coverage(hash_len) == 0 {
            Self::Separate
        } else if left.centers_run(right) || right.centers_run(left) {
            Self::Centered
        } else if left.contains(right) || right.contains(left) {
            Self::Contained
        } else {
            Self::Overlap
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Single => "単独",
            Self::Separate => "離れた複合",
            Self::Overlap => "重なる複合",
            Self::Contained => "入れ子の複合",
            Self::Centered => "中心対称の複合",
            Self::Triple => "三重複合",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Candidate {
    ids: [usize; 3],
    count: usize,
    relation: Relation,
}

impl Candidate {
    fn new(ids: &[usize], relation: Relation) -> Self {
        let mut selected = [0; 3];
        selected[..ids.len()].copy_from_slice(ids);
        Self {
            ids: selected,
            count: ids.len(),
            relation,
        }
    }

    fn label(self, patterns: &[Pattern], text: &str, show_focus: bool) -> String {
        let mut labels = self.ids[..self.count]
            .iter()
            .map(|&id| patterns[id].label(text, show_focus))
            .collect::<Vec<_>>()
            .join(" + ");
        if self.count > 1 {
            write!(labels, " [{}]", self.relation.label()).expect("write to String");
        }
        labels
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LocalScore {
    candidate: Candidate,
    bits: f64,
    pub fixed_bits: f64,
    pub placements: u32,
}

#[derive(Clone, Copy, Debug)]
struct VisualScore {
    candidate: Candidate,
    points: u8,
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
    visual: Option<VisualScore>,
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
        self.local_score()
            .is_some_and(|score| score.candidate.count > 1)
    }

    pub fn primary(&self, text: &str) -> String {
        match self.winner {
            Winner::Local(score) => score.candidate.label(self.patterns, text, true),
            Winner::Frequency { value, count, .. } => {
                format!("文字頻度:{}×{count}", char::from(HEX[value]))
            }
            Winner::AllDigits(_) => "数字のみ".into(),
            Winner::AllLetters(_) => "a-fのみ".into(),
        }
    }

    pub fn beauty_feature(&self, text: &str) -> Option<String> {
        self.visual
            .map(|score| score.candidate.label(self.patterns, text, false))
    }

    pub fn details(&self, text: &str) -> String {
        let mut labels = Vec::new();
        for &pattern in self.patterns {
            let label = pattern.label(text, true);
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
        let (local, visual) = best_local(digits, patterns);
        let local = local.map(Winner::Local);
        let global = self.best_global(digits);
        let winner = match (local, global) {
            (Some(local), Some(global)) if global.bits() > local.bits() => global,
            (Some(local), _) => local,
            (None, global) => global?,
        };
        Some(Assessment {
            patterns,
            winner,
            visual,
            beauty: visual.map_or(0, |score| score.points),
        })
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
        if let Some(category) = category
            && best.is_none_or(|previous| category.bits() > previous.bits())
        {
            best = Some(category);
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

fn best_local(digits: &[u8], patterns: &[Pattern]) -> (Option<LocalScore>, Option<VisualScore>) {
    let mut rarity: Option<LocalScore> = None;
    let mut visual: Option<VisualScore> = None;
    let mut offer = |ids: &[usize], relation: Relation| {
        let mut selected = [patterns[ids[0]]; 3];
        for (pattern, &id) in selected.iter_mut().zip(ids) {
            *pattern = patterns[id];
        }
        let group = &selected[..ids.len()];
        let (fixed_bits, placements) = match relation {
            Relation::Single => (group[0].fixed_bits(), group[0].starts.count_ones()),
            Relation::Separate => (
                group[0].fixed_bits() + group[1].fixed_bits(),
                disjoint_placements(digits.len(), group[0], group[1]),
            ),
            _ => (joint_bits(digits.len(), group), translations(group)),
        };
        let bits = fixed_bits - f64::from(placements).log2();
        debug_assert!(placements > 0);
        if bits > 0.0 && rarity.is_none_or(|best| bits > best.bits + 1e-9) {
            rarity = Some(LocalScore {
                candidate: Candidate::new(ids, relation),
                bits,
                fixed_bits,
                placements,
            });
        }
        if let Some(points) = beauty(digits, group, relation)
            && visual.is_none_or(|best| points > best.points)
        {
            visual = Some(VisualScore {
                candidate: Candidate::new(ids, relation),
                points,
            });
        }
    };

    for (i, &a) in patterns.iter().enumerate() {
        if !a.center_only() {
            offer(&[i], Relation::Single);
        }
    }
    for (i, &a) in patterns.iter().enumerate() {
        for (j, &b) in patterns.iter().enumerate().skip(i + 1) {
            let relation = Relation::between(a, b, digits.len());
            if relation == Relation::Centered || !(a.center_only() || b.center_only()) {
                offer(&[i, j], relation);
            }
        }
    }
    for (i, &parent) in patterns.iter().enumerate() {
        if parent.kind != Kind::Palindrome {
            continue;
        }
        let children = patterns.iter().copied().enumerate().filter(|&(id, child)| {
            id != i && parent.contains(child) && (!child.center_only() || parent.centers_run(child))
        });
        for (j, a) in children.clone() {
            for (k, b) in children.clone().filter(|&(id, _)| id > j) {
                if a.kind == Kind::Run || b.kind == Kind::Run {
                    offer(&[i, j, k], Relation::Triple);
                }
            }
        }
    }
    (rarity, visual)
}

fn beauty(digits: &[u8], group: &[Pattern], relation: Relation) -> Option<u8> {
    // Overlapping descriptions of the same visual kind add no second motif.
    if group.iter().enumerate().any(|(i, a)| {
        group[i + 1..].iter().any(|b| {
            std::mem::discriminant(&a.kind) == std::mem::discriminant(&b.kind)
                && a.coverage(digits.len()) & b.coverage(digits.len()) != 0
        })
    }) {
        return None;
    }

    let base = group
        .iter()
        .map(|pattern| {
            let len = pattern.len;
            match pattern.kind {
                Kind::Run => (5 * len + 9 * usize::from(digits[pattern.start] == 0)).min(68),
                Kind::Repeat(period) => (4 * len + 3 * (len / period)).min(72),
                Kind::Palindrome => (4 * len).min(70),
                Kind::Sequence { .. } => (6 * len).min(68),
                Kind::Word => (7 * len + 10).min(75),
                Kind::Border => (8 * len).min(75),
            }
        })
        .max()
        .unwrap_or(0);
    let bonus = match relation {
        Relation::Single => 0,
        Relation::Separate | Relation::Overlap => 16,
        Relation::Contained => 23,
        Relation::Centered => 37,
        Relation::Triple => 32,
    };
    let centered_zero = group.iter().any(|parent| {
        group
            .iter()
            .any(|&child| parent.centers_run(child) && digits[child.start] == 0)
    });
    Some((base + bonus + 9 * usize::from(centered_zero)).min(100) as u8)
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
    fn centered_three_is_scored_only_as_part_of_its_palindrome() {
        let hash = hash_with("32550005523", 12);
        let text = hash.to_string();
        let mut patterns = Vec::new();
        for (focus, fixed) in [("", 24.0), ("0", 28.0)] {
            let scorer = Scorer::new(focus.parse().unwrap());
            let result = scorer.analyze(&hash, &mut patterns).unwrap();
            assert!((result.bits() - (fixed - 30f64.log2())).abs() < 1e-9);
            assert!(result.primary(&text).contains("ゾロ目:000"));
            assert_eq!(result.beauty, 90);
        }
        let isolated = hash_with("000", 10);
        let scorer = Scorer::new("".parse().unwrap());
        scorer.analyze(&isolated, &mut patterns);
        assert!(!patterns.iter().any(|p| p.center_only()));
    }

    #[test]
    fn a_rarer_unrelated_word_does_not_hide_the_prettier_palindrome() {
        let plain = hash_with("325500005523", 12);
        let mut text = plain.to_string();
        text.replace_range(32..40, "deadbeef");
        let with_word = text.parse().unwrap();
        let mut patterns = Vec::new();
        let scorer = Scorer::new("".parse().unwrap());
        assert_eq!(scorer.analyze(&plain, &mut patterns).unwrap().beauty, 94);
        let result = scorer.analyze(&with_word, &mut patterns).unwrap();
        assert_eq!(result.beauty, 94);
        assert!(result.primary(&text).contains("deadbeef"));
        assert!(result.beauty_feature(&text).unwrap().contains("中心対称"));
    }

    #[test]
    fn focus_changes_only_rarity_and_full_focus_is_neutral() {
        let hash = hash_with("a33333", 34);
        let text = hash.to_string();
        let mut patterns = Vec::new();
        for (focus, rarity) in [("", 15.0), ("3", 19.0), ("0123456789abcdef", 15.0)] {
            let scorer = Scorer::new(focus.parse().unwrap());
            let result = scorer.analyze(&hash, &mut patterns).unwrap();
            assert_eq!(result.bits(), rarity);
            assert_eq!(result.beauty, 25);
            assert_eq!(
                result.beauty_feature(&text).as_deref(),
                Some("ゾロ目:33333")
            );
        }
    }

    #[test]
    fn word_pair_scores_use_disjoint_placements_or_compatible_overlaps() {
        let scorer = Scorer::new("".parse().unwrap());
        let mut patterns = Vec::new();
        for (text, bits, placements) in [
            (
                "d84afeedface95e0a347c125dfacefeed13a2cb8",
                64.0 - 81f64.log2(),
                325,
            ),
            (
                "d84a73bc621ffeedfacefeedd89b6e04f13a2cb8",
                48.0 - 3f64.log2(),
                29,
            ),
        ] {
            let hash = text.parse().unwrap();
            let result = scorer.analyze(&hash, &mut patterns).unwrap();
            let local = result.local_score().unwrap();
            assert!((local.fixed_bits - bits).abs() < 1e-12);
            assert_eq!(local.placements, placements);
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
