use std::{
    cmp::{Ordering, Reverse},
    collections::BinaryHeap,
    fs::File,
    io::{self, BufRead, BufReader, IsTerminal, Read, Seek, Write},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use clap::ValueEnum;
use rayon::{ThreadPool, ThreadPoolBuilder, prelude::*};
use tempfile::NamedTempFile;

use crate::{
    Args,
    hash::{InvalidId, ObjectId},
    pattern::Pattern,
    score::{Explanation, Scorer},
};

const BATCH_BYTES: usize = 1 << 20;
const MAX_LINE_BYTES: usize = 64 + 2;

struct Progress {
    stage: &'static str,
    total: Option<u64>,
    started: Instant,
    updated: Instant,
    terminal: bool,
    line_width: usize,
}

impl Progress {
    fn new(stage: &'static str, total: Option<u64>) -> Self {
        let now = Instant::now();
        let mut progress = Self {
            stage,
            total,
            started: now,
            updated: now,
            terminal: io::stderr().is_terminal(),
            line_width: 0,
        };
        progress.report(0, false);
        progress
    }

    fn update(&mut self, completed: u64) {
        let interval = if self.terminal { 250 } else { 1000 };
        if self.updated.elapsed() >= Duration::from_millis(interval) {
            self.report(completed, false);
        }
    }

    fn finish(&mut self, completed: u64) {
        self.total = Some(completed);
        self.report(completed, true);
    }

    fn report(&mut self, completed: u64, finished: bool) {
        let elapsed = self.started.elapsed().as_secs_f64();
        let status = if let Some(total) = self.total {
            let percent = if finished {
                100.0
            } else if total == 0 {
                0.0
            } else {
                (100.0 * completed as f64 / total as f64).min(99.9)
            };
            let remaining = total.saturating_sub(completed);
            let eta = if finished {
                "0.0s".into()
            } else if completed == 0 {
                "--".into()
            } else {
                format!("{:.1}s", elapsed * remaining as f64 / completed as f64)
            };
            format!(
                "{} {percent:5.1}% ({completed}/{total})  {elapsed:.1}s  ETA {eta}",
                self.stage
            )
        } else {
            format!("{}  {completed} hashes  {elapsed:.1}s", self.stage)
        };
        let mut out = io::stderr().lock();
        // Progress output must not stop a scan if stderr has been closed.
        if self.terminal {
            let width = self.line_width.max(status.len());
            let _ = write!(out, "\r{status:width$}");
            self.line_width = width;
            if finished {
                let _ = writeln!(out);
                self.line_width = 0;
            }
        } else {
            let _ = writeln!(out, "{status}");
        }
        let _ = out.flush();
        self.updated = Instant::now();
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        if self.line_width != 0 {
            let _ = writeln!(io::stderr().lock());
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum SortBy {
    Rarity,
    Beauty,
    Hybrid,
}

impl SortBy {
    fn rank(self, bits: f64, beauty: u8, weight: f64) -> f64 {
        match self {
            Self::Rarity => bits,
            Self::Beauty => f64::from(beauty),
            Self::Hybrid => bits + f64::from(beauty) * weight,
        }
    }
}

#[derive(Clone, Copy)]
struct Key {
    rank: f64,
    bits: f64,
    beauty: u8,
    hash: ObjectId,
}

impl Key {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rank
            .total_cmp(&other.rank)
            .then_with(|| self.bits.total_cmp(&other.bits))
            .then_with(|| self.beauty.cmp(&other.beauty))
            .then_with(|| self.hash.digits().cmp(other.hash.digits()))
    }
}

struct Finding {
    key: Key,
    composite: bool,
    primary: Explanation,
    features: String,
    beauty_feature: Option<Explanation>,
    additional_features: Vec<String>,
}

impl Ord for Finding {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key)
    }
}

impl PartialOrd for Finding {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Finding {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Finding {}

impl Finding {
    fn write_csv(&self, writer: &mut csv::Writer<NamedTempFile>) -> io::Result<()> {
        let rank = format!("{:.4}", self.key.rank);
        let bits = format!("{:.4}", self.key.bits);
        let beauty = self.key.beauty.to_string();
        let hash = self.key.hash.to_string();
        let primary = self.primary.to_string();
        let beauty_feature = self
            .beauty_feature
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default();
        writer
            .write_record([
                rank.as_str(),
                &bits,
                &beauty,
                if self.composite { "true" } else { "false" },
                &hash,
                &primary,
                &self.features,
                &beauty_feature,
            ])
            .map_err(io::Error::other)
    }
}

#[derive(Default)]
struct Counts {
    scanned: u64,
    scanned64: u64,
    matched: u64,
    composite: u64,
}

impl Counts {
    fn expected(&self, key: Key) -> f64 {
        let count = if key.hash.digits().len() == 64 {
            self.scanned64
        } else {
            self.scanned - self.scanned64
        };
        count as f64 * 2.0f64.powf(-key.bits)
    }
}

fn format_expected(value: f64) -> String {
    if value < 0.000001 {
        return "<0.000001".into();
    }
    let decimals = if value < 1.0 { 6 } else { 2 };
    let decimal = format!("{value:.decimals$}");
    decimal.trim_end_matches('0').trim_end_matches('.').into()
}

#[derive(Default)]
struct Worker {
    patterns: Vec<Pattern>,
    findings: Vec<Finding>,
}

impl Worker {
    fn scan(
        &mut self,
        chunk: &[u8],
        args: &Args,
        scorer: &Scorer,
        cutoff: Option<Key>,
    ) -> Result<Counts, (usize, InvalidId)> {
        let mut counts = Counts::default();
        let chunk = chunk.strip_suffix(b"\n").unwrap_or(chunk);
        let save_csv = args.csv.is_some();
        for (index, line) in chunk.split(|&byte| byte == b'\n').enumerate() {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            let hash = ObjectId::parse(line).map_err(|error| (index, error))?;
            counts.scanned += 1;
            counts.scanned64 += u64::from(line.len() == 64);
            let Some(result) = scorer.analyze(&hash, &mut self.patterns) else {
                continue;
            };
            let bits = result.bits();
            if bits < args.min_bits {
                continue;
            }
            let composite = result.composite();
            counts.matched += 1;
            counts.composite += u64::from(composite);
            let key = Key {
                rank: args.sort.rank(bits, result.beauty, args.beauty_weight),
                bits,
                beauty: result.beauty,
                hash,
            };
            if !save_csv && cutoff.is_some_and(|worst| key.cmp(&worst) != Ordering::Greater) {
                continue;
            }
            let text = hash.to_string();
            self.findings.push(Finding {
                key,
                composite,
                primary: result.primary(&text),
                beauty_feature: result.beauty_feature(&text),
                features: if save_csv {
                    result.details(&text)
                } else {
                    String::new()
                },
                additional_features: if args.details {
                    result.additional_features(&text)
                } else {
                    Vec::new()
                },
            });
        }
        Ok(counts)
    }
}

pub struct Summary {
    counts: Counts,
    top: BinaryHeap<Reverse<Finding>>,
}

impl Summary {
    pub fn write(self, mut out: impl Write, args: &Args, elapsed: Duration) -> io::Result<()> {
        let sort = args.sort.to_possible_value().expect("visible sort modes");
        writeln!(out, "Scanned             : {}", self.counts.scanned)?;
        writeln!(
            out,
            "Matches (>= {:.1} bits): {}",
            args.min_bits, self.counts.matched
        )?;
        writeln!(out, "Composite winners   : {}", self.counts.composite)?;
        writeln!(
            out,
            "Threads / elapsed   : {} / {:.3} s",
            args.threads,
            elapsed.as_secs_f64()
        )?;
        writeln!(
            out,
            "Sort                : {} (beauty weight={})",
            sort.get_name(),
            args.beauty_weight
        )?;
        if args.focus_digits.count() != 0 {
            writeln!(out, "Focus digits        : {}", args.focus_digits)?;
        }
        if self.top.is_empty() {
            writeln!(out, "\nNo matching hashes.")?;
        }
        for (index, Reverse(finding)) in self.top.into_sorted_vec().into_iter().enumerate() {
            let key = finding.key;
            let expected = format_expected(self.counts.expected(key));
            writeln!(out, "\n#{}  {}", index + 1, key.hash)?;
            writeln!(
                out,
                "    Score {:.2}   Rarity {:.2} bits   Beauty {}/100",
                key.rank, key.bits, key.beauty
            )?;
            writeln!(out, "    Expected {expected} hits")?;
            write_explanation(&mut out, "Rarity", &finding.primary)?;
            if let Some(beauty) = finding
                .beauty_feature
                .as_ref()
                .filter(|&feature| feature != &finding.primary)
            {
                write_explanation(&mut out, "Beauty", beauty)?;
            }
            if !finding.additional_features.is_empty() {
                writeln!(out, "    Other")?;
                for feature in finding.additional_features {
                    writeln!(out, "      - {feature}")?;
                }
            }
        }
        if let Some(path) = &args.csv {
            writeln!(out, "\nCSV saved to {}", path.display())?;
        }
        writeln!(
            out,
            "\nExpected is a bound for the shown pattern family.\nIt uses the number of scanned hashes of the same length."
        )
    }
}

fn write_explanation(
    mut out: impl Write,
    title: &str,
    explanation: &Explanation,
) -> io::Result<()> {
    if let Some(relation) = explanation.relation {
        writeln!(out, "    {title} ({relation})")?;
        for part in &explanation.parts {
            writeln!(out, "      - {part}")?;
        }
        Ok(())
    } else {
        writeln!(out, "    {title}  {}", explanation.parts[0])
    }
}

pub fn run(args: &Args, scorer: &Scorer) -> io::Result<Summary> {
    let pool = ThreadPoolBuilder::new()
        .num_threads(usize::from(args.threads))
        .build()
        .map_err(io::Error::other)?;
    let mut csv = args.csv.as_deref().map(open_csv).transpose()?;
    let (reader, total): (Box<dyn BufRead>, Option<u64>) = if args.stdin {
        (Box::new(io::stdin().lock()), None)
    } else {
        let mut progress = Progress::new("Reading Git history", None);
        let mut command = Command::new("git");
        if let Some(path) = &args.repo {
            command.arg("-C").arg(path);
        }
        let mut git = command
            .args(["rev-list", "--all"])
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_NO_LAZY_FETCH", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| {
                io::Error::new(
                    error.kind(),
                    format!("could not start git rev-list: {error}"),
                )
            })?;
        let reader = BufReader::new(git.stdout.take().expect("piped Git stdout"));
        let result = snapshot(reader, |count| progress.update(count));
        if result.is_err() {
            let _ = git.kill();
        }
        let status = git.wait();
        let (file, total) = result?;
        let status = status?;
        if !status.success() {
            return Err(io::Error::other(format!("git rev-list failed: {status}")));
        }
        progress.finish(total);
        (Box::new(BufReader::new(file)), Some(total))
    };
    let mut progress = Progress::new("Scoring", total);
    let summary = scan(reader, args, scorer, &pool, csv.as_mut(), &mut progress)?;
    if let (Some(csv), Some(path)) = (csv, &args.csv) {
        let temp = csv.into_inner().map_err(|error| error.into_error())?;
        temp.persist(path).map_err(|error| error.error)?;
    }
    progress.finish(summary.counts.scanned);
    Ok(summary)
}

fn snapshot(mut reader: impl BufRead, mut report: impl FnMut(u64)) -> io::Result<(File, u64)> {
    let mut file = tempfile::tempfile()?;
    let mut batch = Vec::with_capacity(BATCH_BYTES + MAX_LINE_BYTES + 1);
    let mut total = 0;
    let mut unterminated = false;
    loop {
        read_batch(&mut reader, &mut batch)?;
        if batch.is_empty() {
            break;
        }
        total += batch.iter().filter(|&&byte| byte == b'\n').count() as u64;
        unterminated = !batch.ends_with(b"\n");
        file.write_all(&batch)?;
        report(total);
    }
    total += u64::from(unterminated);
    file.rewind()?;
    Ok((file, total))
}

fn open_csv(path: &Path) -> io::Result<csv::Writer<NamedTempFile>> {
    if path.file_name().is_none() || path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("expected a CSV file path: {}", path.display()),
        ));
    }
    let parent = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = NamedTempFile::new_in(parent)?;
    temp.write_all(b"\xef\xbb\xbf")?;
    let mut writer = csv::Writer::from_writer(temp);
    writer
        .write_record([
            "rank",
            "bits",
            "beauty",
            "composite",
            "hash",
            "primary",
            "features",
            "beauty_feature",
        ])
        .map_err(io::Error::other)?;
    Ok(writer)
}

fn scan(
    mut reader: impl BufRead,
    args: &Args,
    scorer: &Scorer,
    pool: &ThreadPool,
    mut csv: Option<&mut csv::Writer<NamedTempFile>>,
    progress: &mut Progress,
) -> io::Result<Summary> {
    let mut summary = Summary {
        counts: Counts::default(),
        top: BinaryHeap::new(),
    };
    let thread_count = usize::from(args.threads);
    let limit = args.top as usize;
    let mut workers: Vec<_> = (0..thread_count).map(|_| Worker::default()).collect();
    let mut results = Vec::with_capacity(thread_count);
    let mut batch = Vec::with_capacity(BATCH_BYTES + MAX_LINE_BYTES + 1);
    loop {
        read_batch(&mut reader, &mut batch)?;
        if batch.is_empty() {
            break;
        }
        let chunks = split_batch(&batch, thread_count);
        let chunk_count = chunks.len();
        let cutoff = if summary.top.len() == limit {
            summary.top.peek().map(|worst| worst.0.key)
        } else {
            None
        };
        pool.install(|| {
            workers
                .par_iter_mut()
                .zip(chunks)
                .map(|(worker, chunk)| worker.scan(chunk, args, scorer, cutoff))
                .collect_into_vec(&mut results);
        });
        for result in results.drain(..) {
            let counts = result.map_err(|(index, error)| {
                let line = summary.counts.scanned + index as u64 + 1;
                io::Error::new(io::ErrorKind::InvalidData, format!("line {line}: {error}"))
            })?;
            summary.counts.scanned += counts.scanned;
            summary.counts.scanned64 += counts.scanned64;
            summary.counts.matched += counts.matched;
            summary.counts.composite += counts.composite;
        }
        for worker in workers.iter_mut().take(chunk_count) {
            for finding in worker.findings.drain(..) {
                if let Some(writer) = csv.as_deref_mut() {
                    finding.write_csv(writer)?;
                }
                if summary.top.len() < limit {
                    summary.top.push(Reverse(finding));
                } else if let Some(mut worst) = summary.top.peek_mut()
                    && finding > worst.0
                {
                    *worst = Reverse(finding);
                }
            }
        }
        progress.update(summary.counts.scanned);
    }
    Ok(summary)
}

fn read_batch(reader: &mut impl BufRead, batch: &mut Vec<u8>) -> io::Result<()> {
    batch.clear();
    reader
        .by_ref()
        .take(BATCH_BYTES as u64)
        .read_to_end(batch)?;
    if !batch.is_empty() && !batch.ends_with(b"\n") {
        // Finish the split line, with room for one extra byte to reject overlong input.
        reader
            .take((MAX_LINE_BYTES + 1) as u64)
            .read_until(b'\n', batch)?;
    }
    Ok(())
}

fn split_batch(mut batch: &[u8], parts: usize) -> Vec<&[u8]> {
    let target = batch.len().div_ceil(parts);
    let mut chunks = Vec::with_capacity(parts);
    while batch.len() > target {
        let end = batch[target - 1..]
            .iter()
            .position(|&byte| byte == b'\n')
            .map_or(batch.len(), |offset| target + offset);
        let (chunk, rest) = batch.split_at(end);
        chunks.push(chunk);
        batch = rest;
    }
    if !batch.is_empty() {
        chunks.push(batch);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn snapshot_preserves_line_endings_and_counts_a_final_unterminated_hash() {
        let short = "d84a73bc621f325500005523d89b6e04f13a2cb8";
        let long = "0123456789abcdef".repeat(4);
        for input in [
            String::new(),
            format!("{short}\r\n{long}"),
            format!("{}{long}", format!("{short}\n").repeat(30_000)),
        ] {
            let expected = input.lines().count() as u64;
            let (mut file, count) = snapshot(Cursor::new(input.as_bytes()), |_| {}).unwrap();
            let mut copied = String::new();
            file.read_to_string(&mut copied).unwrap();
            assert_eq!(copied, input);
            assert_eq!(count, expected);
        }
    }
}
