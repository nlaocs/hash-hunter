mod constraints;
mod hash;
mod pattern;
mod scan;
mod score;

use std::{
    io::{self, BufWriter, Write},
    path::PathBuf,
    process::ExitCode,
    thread,
    time::Instant,
};

use clap::Parser;

use crate::{
    hash::{FocusDigits, ObjectId},
    scan::{Column, SortBy},
    score::Scorer,
};

#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Read Git history from PATH (defaults to the current directory)
    #[arg(long, value_name = "PATH", conflicts_with = "stdin")]
    repo: Option<PathBuf>,

    /// Read lowercase 40- or 64-digit hashes from standard input, one per line
    #[arg(long)]
    stdin: bool,

    /// Explain the score for one 40- or 64-digit hash
    #[arg(long, value_name = "HASH", conflicts_with_all = ["repo", "stdin"])]
    explain: Option<ObjectId>,

    /// Number of scoring threads
    #[arg(long, default_value_t = default_threads(), value_parser = clap::value_parser!(u8).range(1..=64))]
    threads: u8,

    /// Number of ranked results to print
    #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u32).range(1..=100_000))]
    top: u32,

    /// Minimum Rarity in bits
    #[arg(long, default_value_t = 12.0, value_parser = parse_min_bits)]
    min_bits: f64,

    /// Score used to rank results
    #[arg(long, value_enum, default_value = "hybrid")]
    sort: SortBy,

    /// Weight of Beauty in the Hybrid score
    #[arg(long, default_value_t = 0.12, value_parser = parse_weight)]
    beauty_weight: f64,

    /// Preselect digits for the focused-run pattern family
    #[arg(long, default_value = "", value_name = "HEX")]
    focus_digits: FocusDigits,

    /// Save every match to a UTF-8 CSV file with BOM
    #[arg(long, value_name = "PATH")]
    csv: Option<PathBuf>,

    /// Columns to show in the one-line list, in the given order
    #[arg(long, value_enum, value_delimiter = ',', default_value = "score,hash,pattern", value_name = "FIELDS", conflicts_with_all = ["details", "explain"])]
    columns: Vec<Column>,

    /// Show detailed multi-line scores, reasons, and additional features
    #[arg(long)]
    details: bool,
}

fn default_threads() -> u8 {
    thread::available_parallelism().map_or(1, |count| count.get().min(8) as u8)
}

fn parse_min_bits(text: &str) -> Result<f64, String> {
    let value = text.parse::<f64>().map_err(|error| error.to_string())?;
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err("expected a finite non-negative value".into())
    }
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
    let primary = result.primary(&text);
    writeln!(out, "Rarity pattern: {primary}")?;
    if let Some(beauty) = result
        .beauty_feature(&text)
        .filter(|feature| feature != &primary)
    {
        writeln!(out, "Beauty pattern: {beauty}")?;
    }
    if let Some(score) = result.local_score() {
        writeln!(out, "Fixed layout : {:.3} bits", score.fixed_bits)?;
        writeln!(
            out,
            "Placements   : {} (log2={:.3})",
            score.placements,
            f64::from(score.placements).log2()
        )?;
        writeln!(out, "Composite    : {}", result.composite())?;
    }
    let mut features = result.patterns.to_vec();
    features.sort_by(|a, b| b.bits().total_cmp(&a.bits()));
    writeln!(out, "\nDetected local features ({})", features.len())?;
    for feature in features {
        let label = feature.label(&text, true);
        if feature.center_only() {
            writeln!(out, "  center-only  {label}")?;
        } else {
            writeln!(out, "  {:>7.2} bits  {label}", feature.bits())?;
        }
    }
    writeln!(
        out,
        "\nRarity uses a probability bound for the reported pattern family."
    )
}

fn run(args: &Args) -> io::Result<()> {
    let started = Instant::now();
    let scorer = Scorer::new(args.focus_digits);
    let mut out = BufWriter::new(io::stdout().lock());
    if let Some(hash) = &args.explain {
        explain(&mut out, hash, &scorer, args.beauty_weight)?;
    } else {
        let summary = scan::run(args, &scorer)?;
        summary.write(&mut out, args, started.elapsed())?;
    }
    out.flush()
}

fn main() -> ExitCode {
    match run(&Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}
