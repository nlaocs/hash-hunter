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
    let output = run_stdin(binary().arg("--csv").arg(&path), hash.as_bytes());
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout
            .lines()
            .any(|line| line.trim_start().starts_with("Beauty  ") && line.contains("中心対称"))
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
    let output = run_stdin(binary().args(["--threads", "4"]), input.as_bytes());
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    for (hash, expected) in [(long, "0.000015"), (FIRST, "<0.000001")] {
        let row = stdout.lines().find(|line| line.contains(hash)).unwrap();
        assert_eq!(row.split_whitespace().nth(3), Some(expected));
    }
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
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "previous results\n"
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}
