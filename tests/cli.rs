use std::{
    io::Write,
    process::{Command, Output, Stdio},
};

const FIRST: &str = "d84a73bc621f325500005523d89b6e04f13a2cb8";
const SECOND: &str = "d84a73bc621f325500005523d89b6e04f13a2cb9";

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_hash-hunter"))
}

fn run_stdin(command: &mut Command, input: &[u8]) -> Output {
    let mut child = command
        .arg("--stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn rejects_conflicting_modes_and_invalid_values() {
    for args in [
        &["--repo", ".", "--stdin"][..],
        &["--stdin", "--explain", FIRST],
        &["--threads", "0"],
        &["--min-bits", "inf"],
        &["--beauty-weight", "NaN"],
        &["--focus-digits", "F"],
        &["--columns", "unknown"],
        &["--columns", ""],
        &["--columns", "hash", "--details"],
        &["--columns", "hash", "--explain", FIRST],
    ] {
        let output = binary().args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}");
    }
}

#[test]
fn top_ties_use_the_same_order_for_each_thread_count() {
    for threads in ["1", "4"] {
        for (left, right) in [(FIRST, SECOND), (SECOND, FIRST)] {
            let input = format!("{left}\n{right}\n");
            let output = run_stdin(
                binary().args(["--top", "1", "--threads", threads]),
                input.as_bytes(),
            );
            assert!(output.status.success(), "{output:?}");
            let stdout = String::from_utf8(output.stdout).unwrap();
            assert!(stdout.contains(SECOND));
            assert!(!stdout.contains(FIRST));
        }
    }
}

#[test]
fn csv_keeps_all_matches_in_input_order_for_mixed_hash_lengths() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("findings.csv");
    let long = "0123456789abcdef".repeat(4);
    let input = format!("{FIRST}\r\n{long}");
    let output = run_stdin(
        binary()
            .args(["--threads", "4", "--top", "1", "--csv"])
            .arg(&path),
        input.as_bytes(),
    );
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Scoring  0 hashes"));
    assert!(stderr.contains("100.0% (2/2)"));
    let data = std::fs::read(path).unwrap();
    let payload = data.strip_prefix(b"\xef\xbb\xbf").unwrap();
    let mut reader = csv::Reader::from_reader(payload);
    assert_eq!(
        reader.headers().unwrap(),
        [
            "rank",
            "bits",
            "beauty",
            "composite",
            "hash",
            "primary",
            "features",
            "beauty_feature"
        ]
        .as_slice()
    );
    let hashes: Vec<_> = reader
        .records()
        .map(|row| row.unwrap()[4].to_owned())
        .collect();
    assert_eq!(hashes, [FIRST, long.as_str()]);
}

#[test]
fn explain_and_csv_show_the_two_scoring_reasons() {
    let hash = "d84a73bc621f325500005523d89b6e04deadbeef";
    let output = binary().args(["--explain", hash]).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout
            .lines()
            .any(|line| line.starts_with("Rarity pattern:") && line.contains("deadbeef"))
    );
    assert!(
        stdout
            .lines()
            .any(|line| line.starts_with("Beauty pattern:") && line.contains("中心対称"))
    );

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("findings.csv");
    let output = run_stdin(
        binary().args(["--details", "--csv"]).arg(&path),
        hash.as_bytes(),
    );
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout
            .lines()
            .any(|line| line.trim_start().starts_with("Beauty (") && line.contains("中心対称"))
    );
    let row = csv::Reader::from_path(path)
        .unwrap()
        .records()
        .next()
        .unwrap()
        .unwrap();
    assert!(row[5].contains("deadbeef"));
    assert!(row[7].contains("中心対称"));
}

#[test]
fn expected_is_decimal_and_counts_only_the_same_hash_length() {
    let long = "112290d5a738e77b1e2034bb2830687c91b29c5e1a6f1172ac07b2e4da751122";
    let input = format!("{FIRST}\n{long}\n");
    let output = run_stdin(
        binary().args(["--threads", "4", "--columns", "expected,hash"]),
        input.as_bytes(),
    );
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    for (hash, expected) in [(long, "0.000015"), (FIRST, "<0.000001")] {
        let line = stdout.lines().find(|line| line.contains(hash)).unwrap();
        assert_eq!(
            line.split_whitespace().collect::<Vec<_>>(),
            [expected, hash]
        );
    }
}

#[test]
fn default_list_keeps_one_line_per_hash_without_extra_beauty_reasons() {
    let composite = "23b6ee5cd9d99d9d99d92ff923d6971a33dc7b03";
    let different_beauty = "d84a73bc621f325500005523d89b6e04deadbeef";
    let input = format!("{composite}\n{different_beauty}\n");
    let output = run_stdin(&mut binary(), input.as_bytes());
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let rows: Vec<_> = stdout
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("Score  "))
        .collect();
    assert_eq!(rows.len(), 3, "{stdout}");
    assert_eq!(
        rows[0].split_whitespace().collect::<Vec<_>>(),
        ["Score", "Hash", "Pattern"]
    );
    for hash in [composite, different_beauty] {
        assert_eq!(
            rows[1..].iter().filter(|line| line.contains(hash)).count(),
            1
        );
    }
    assert!(!stdout.contains("Beauty"));
    assert!(!stdout.contains("Expected"));
    assert!(!stdout.contains("    Other"));
}

#[test]
fn columns_select_fields_in_the_requested_order() {
    let output = run_stdin(
        binary().args(["--columns", "hash,beauty,score,bits,expected"]),
        FIRST.as_bytes(),
    );
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let header = stdout
        .lines()
        .find(|line| line.starts_with("Hash "))
        .unwrap();
    assert_eq!(
        header.split_whitespace().collect::<Vec<_>>(),
        ["Hash", "Beauty", "Score", "Bits", "Expected"]
    );
    let row = stdout.lines().find(|line| line.contains(FIRST)).unwrap();
    assert_eq!(
        row.split_whitespace().collect::<Vec<_>>(),
        [FIRST, "94", "34.42", "23.14", "<0.000001"]
    );
    assert!(!stdout.contains("回文:"));
}

#[test]
fn details_keep_grouped_reasons_and_only_additional_features() {
    let hash = "23b6ee5cd9d99d9d99d92ff923d6971a33dc7b03";
    let output = run_stdin(binary().args(["--top", "1", "--details"]), hash.as_bytes());
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains(&format!("#1  {hash}\n")));
    assert!(stdout.contains("    Score 40.38   Rarity 31.14 bits   Beauty 77/100\n"));
    assert!(stdout.contains(
        "    Rarity (入れ子の複合)\n      - 周期5:d9d99d9d99d9\n      - 回文:9d99d9d99d9\n"
    ));
    assert!(stdout.contains("    Other\n      - 回文:d9d99d9d\n"));
    assert_eq!(stdout.matches("周期5:d9d99d9d99d9").count(), 1);
    assert_eq!(stdout.matches("回文:9d99d9d99d9").count(), 1);
}

#[test]
fn git_scan_reports_progress_with_a_known_total() {
    let repo = tempfile::tempdir().unwrap();
    let init = Command::new("git")
        .args(["init", "--quiet", "--template="])
        .arg(repo.path())
        .output()
        .unwrap();
    assert!(init.status.success(), "{init:?}");
    let empty = binary().arg("--repo").arg(repo.path()).output().unwrap();
    assert!(empty.status.success(), "{empty:?}");
    let stderr = String::from_utf8(empty.stderr).unwrap();
    assert!(stderr.contains("100.0% (0/0)"));
    assert!(!stderr.contains("NaN"));
    let commit = Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
        ])
        .arg(format!("core.hooksPath={}", repo.path().display()))
        .args(["commit", "--allow-empty", "--quiet", "-m", "fixture"])
        .output()
        .unwrap();
    assert!(commit.status.success(), "{commit:?}");
    let output = binary().arg("--repo").arg(repo.path()).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Reading Git history"));
    assert!(
        stderr
            .lines()
            .any(|line| line.starts_with("Scoring ") && line.contains("0.0% (0/1)"))
    );
    assert!(
        stderr
            .lines()
            .any(|line| line.starts_with("Scoring ") && line.contains("100.0% (1/1)"))
    );
    assert!(!String::from_utf8(output.stdout).unwrap().contains("100.0%"));
}

#[test]
fn invalid_line_after_a_batch_preserves_existing_csv() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("findings.csv");
    std::fs::write(&path, "previous results\n").unwrap();
    let filler = "d84a73bc621f95e0a347c125d89b6e04f13a2cb8\n";
    let input = format!("{FIRST}\n{}invalid\n", filler.repeat(26_000));
    let output = run_stdin(
        binary().args(["--threads", "4", "--csv"]).arg(&path),
        input.as_bytes(),
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("line 26002"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("100.0%"));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "previous results\n"
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}
