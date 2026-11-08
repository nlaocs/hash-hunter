# Hash Hunter

Hash Hunter is a CLI for finding palindromes, runs, and repeats in Git commit hashes. It supports SHA-1 and SHA-256 and ranks hashes by rarity, visual structure, or both.

[![Version 0.2.0](https://img.shields.io/badge/version-0.2.0-2563eb)](Cargo.toml) [![Rust 2024](https://img.shields.io/badge/Rust-2024-24292f?logo=rust&logoColor=white)](Cargo.toml) [![MIT License](https://img.shields.io/badge/license-MIT-15803d)](LICENSE)

[Features](#features) · [Installation](#installation) · [Usage](#usage) · [Scoring](#scoring) · [Development](#development) · [License](#license)

[日本語](README.md) · [English](README.en.md) · [한국어](README.ko.md)

---

## Features

- Read Git history or a hash list in batches and score hashes in parallel on the CPU.
- Detect palindromes, runs, repeats with periods from 2 to 8, non-wrapping hexadecimal sequences, dictionary words such as `deadbeef`, and matching prefixes and suffixes. The scanner also looks for unusually frequent digits and hashes made entirely of digits or `a-f`.
- Inspect the top results and their scoring reasons in the terminal. Export every match that meets the threshold to CSV.

## Installation

Use stable Rust and Cargo. Repository scans use Git from your PATH.

```sh
git clone https://github.com/nlaocs/hash-hunter.git
cd hash-hunter
cargo install --path . --locked
```

## Usage

### Scan Git history

```sh
hash-hunter --repo "path/to/repository" --top 100
hash-hunter --repo "path/to/repository" --sort rarity --min-bits 16 --csv findings.csv
```

With no `--repo` argument, Hash Hunter uses the repository in the current directory. It runs `git rev-list --all` to read commits reachable from local refs and HEAD.

Progress goes to standard error. `Reading Git history` shows the number of hashes read and elapsed time. `Scoring` shows the percentage, processed count, elapsed time, and estimated time remaining (ETA). Elapsed time keeps updating while waiting for input. The total is unknown during history enumeration, so percentages start when scoring begins.

To obtain an exact total, Hash Hunter saves Git's hash list to a temporary file before scoring it. Git enumerates the history only once, and the temporary file is deleted when closed. A Linux history of about 1.5 million commits uses about 62 MB of temporary storage.

The default list uses one line per hash, showing the score, hash, and primary pattern. Beauty reasons and additional features do not interrupt the list. This excerpt comes from a scan of the Linux history.

```text
Score  Hash                                      Pattern
40.38  23b6ee5cd9d99d9d99d92ff923d6971a33dc7b03  周期5:d9d99d9d99d9 + 回文:9d99d9d99d9
38.92  5f5837d2d650db25b9153b91535e67a96b265f58  周期5:b9153b9153 + 先頭=末尾:5f58
```

Use `--columns` to select and order fields. Choose from `score`, `bits`, `beauty`, `expected`, `hash`, and `pattern`, separated by commas. Column selection does not change scoring or ranking.

```sh
hash-hunter --repo "path/to/repository" --columns score,hash
hash-hunter --repo "path/to/repository" --columns score,bits,beauty,expected,hash,pattern
hash-hunter --repo "path/to/repository" --details
```

`--details` groups scores and full reasons for each hash across multiple lines. Composite patterns appear on separate lines, followed by features not already shown in either scoring reason. It cannot be combined with `--columns`.

```text
#1  23b6ee5cd9d99d9d99d92ff923d6971a33dc7b03
    Score 40.38   Rarity 31.14 bits   Beauty 77/100
    Expected 0.000635 hits
    Rarity (入れ子の複合)
      - 周期5:d9d99d9d99d9
      - 回文:9d99d9d99d9
    Other
      - 回文:d9d99d9d
```

CSV rows follow input order and use UTF-8 with a BOM. If the destination exists, Hash Hunter replaces it after the scan and write succeed.

### Read a hash list

Pass one lowercase 40- or 64-digit hexadecimal hash per line to `--stdin`. Both LF and CRLF work. In PowerShell, run the following command.

```powershell
Get-Content .\hashes.txt | hash-hunter --stdin --threads 8
```

`--stdin` scores hashes as it reads them and shows the processed count and elapsed time. Since the total is unknown, it displays 100% only on completion.

### Explain one score

```sh
hash-hunter --explain d84a73bc621f325500005523d89b6e04f13a2cb8
```

An excerpt from the output follows.

```text
Rarity       : 23.142 bits
Beauty       : 94 / 100 (heuristic)
Hybrid       : 34.422 (weight=0.12)
Fixed layout : 28.000 bits
Placements   : 29 (log2=4.858)
```

The hash contains the palindrome `325500005523`. The palindrome contributes 24 bits and the central `0000` adds 4 bits of independent constraints. Accounting for the 29 placements in a 40-digit hash gives about 23.14 bits of Rarity.

### Options

| Option | Purpose | Default |
| --- | --- | --- |
| `--repo PATH` | Select a Git repository | Current directory |
| `--stdin` | Read a hash list from standard input | — |
| `--explain HASH` | Explain one score | — |
| `--top N` | Number of top results to print | 50 |
| `--threads N` | Scoring threads | Available logical CPUs, capped at 8 |
| `--min-bits X` | Minimum Rarity for a match | 12 bits |
| `--sort MODE` | Rank by `rarity`, `beauty`, or `hybrid` | `hybrid` |
| `--beauty-weight X` | Beauty coefficient in the Hybrid score | 0.12 |
| `--focus-digits HEX` | Score runs of selected digits as a separate pattern family | None |
| `--columns FIELDS` | Select and order list fields, separated by commas | `score,hash,pattern` |
| `--details` | Show scores, reasons, and additional features across multiple lines | — |
| `--csv PATH` | Save all matches that meet the threshold | — |
| `--help` / `--version` | Show help or the version | — |

### Scan large histories repeatedly

A Git commit-graph can speed up history enumeration. This command writes the index to the repository's Git metadata. [Git documentation](https://git-scm.com/docs/git-commit-graph)

```sh
git -C "path/to/repository" commit-graph write --reachable
```

To try different scoring options on the same history, save the output of `git rev-list --all` once and pass it to `--stdin`.

## Scoring

- **Rarity** is `-log2(q)` bits, where `q` is an upper bound on a pattern family's occurrence probability. The calculation assumes independent uniform hex digits. For separate pairs, it counts disjoint placements, with each pair of locations counted once if both patterns belong to the same family. For overlaps, it fixes the relative layout and counts compatible combinations of words and sequences.
- **Beauty** scores length, symmetry, and other visual properties on a scale of 0 to 100. Overlapping features of the same kind count as one visual motif. Rarity and Beauty each select their highest-scoring candidate. `--details` and `--explain` show both reasons when they differ. In CSV, `primary` gives the Rarity reason and `beauty_feature` gives the Beauty reason. `composite` indicates whether the Rarity reason combines patterns.
- **Hybrid** is `Rarity + beauty-weight × Beauty`. Ranking ties go to the higher Rarity, then higher Beauty, then the lexicographically greater hash.
- **Expected** is an upper bound on the expected number of matches for the displayed family. It multiplies `2^(-Rarity)` by the number of scanned hashes of the same length. The display uses decimals and shows values below one millionth as `<0.000001`.

Runs at the ends and runs in the interior form separate families. A run of three at the center of an odd-length palindrome can be scored together with that palindrome. `--focus-digits` affects Rarity. The scanner keeps at most 24 local candidates per hash by Rarity before the focus adjustment, breaking ties by detection order. It scores single patterns, pairs, and triples that combine a palindrome with two contained patterns, at least one of which is a run.

## Development

```sh
cargo build --release --locked
cargo fmt --all -- --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

## License

Hash Hunter is available under the MIT License. [LICENSE](LICENSE)
