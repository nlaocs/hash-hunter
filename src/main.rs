mod constraints;
mod hash;
mod pattern;
mod score;

use std::{
    io::{self, Write},
    process::ExitCode,
};

use clap::Parser;

use crate::{
    hash::{FocusDigits, ObjectId},
    score::Scorer,
};

#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Explain the score for one 40- or 64-digit hash
    #[arg(long, value_name = "HASH")]
    explain: ObjectId,

    /// Weight of Beauty in the Hybrid score
    #[arg(long, default_value_t = 0.12, value_parser = parse_weight)]
    beauty_weight: f64,

    /// Preselect digits for the focused-run pattern family
    #[arg(long, default_value = "", value_name = "HEX")]
    focus_digits: FocusDigits,
}

fn parse_weight(text: &str) -> Result<f64, String> {
    let value = text.parse::<f64>().map_err(|error| error.to_string())?;
    if (0.0..=10.0).contains(&value) {
        Ok(value)
    } else {
        Err("expected a finite value between 0 and 10".into())
    }
}

fn explain(mut out: impl Write, hash: &ObjectId, scorer: &Scorer, weight: f64) -> io::Result<()> {
    writeln!(out, "Hash         : {hash}")?;
    let mut patterns = Vec::new();
    let Some(result) = scorer.analyze(hash, &mut patterns) else {
        return writeln!(out, "No notable pattern detected.");
    };

    let text = hash.to_string();
    writeln!(out, "Rarity       : {:.3} bits", result.bits())?;
    writeln!(out, "Beauty       : {} / 100 (heuristic)", result.beauty)?;
    let hybrid = result.bits() + f64::from(result.beauty) * weight;
    writeln!(out, "Hybrid       : {hybrid:.3} (weight={weight})")?;
    writeln!(out, "Main feature : {}", result.primary(&text))?;
    if let Some(score) = result.local_score() {
        let opportunities = f64::from(score.opportunities);
        writeln!(
            out,
            "Constraints  : {:.3} bits at fixed positions",
            score.fixed_bits
        )?;
        writeln!(
            out,
            "Opportunities: {opportunities:.3} (log2={:.3})",
            opportunities.log2()
        )?;
        writeln!(out, "Composite    : {}", result.composite())?;
    }
    let mut features = result.patterns.to_vec();
    features.sort_by(|a, b| b.bits().total_cmp(&a.bits()));
    writeln!(out, "\nDetected local features ({})", features.len())?;
    for feature in features {
        writeln!(
            out,
            "  {:>7.2} bits  {}",
            feature.bits(),
            feature.label(&text)
        )?;
    }
    writeln!(
        out,
        "\nRarity estimates the occurrence of the reported pattern family."
    )
}

fn main() -> ExitCode {
    let args = Args::parse();
    let scorer = Scorer::new(args.focus_digits);
    match explain(
        io::stdout().lock(),
        &args.explain,
        &scorer,
        args.beauty_weight,
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}
