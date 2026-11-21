//! The fingerprint of the lines of a job: the same plan and profile always give the same one, a place that moves by a thousandth of a millimetre gives another, the fingerprint is the SHA-256 of
//! the lines as they are written to a file, and `--expect` ends with 2 when the lines are not the ones that were authorised. Boards made from a fixed seed give a different fingerprint each.

use eh3d_phoenix_pnp::toolpath::{fingerprint, is_fingerprint, matches, text};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PROFILE: &str = "examples/driver_marlin.cfg";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .current_dir(root())
        .output()
        .unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_tp_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// The fingerprint that the command prints.
fn hash_of(parts: &str, feeders: &str) -> String {
    let o = run(&["toolpath-hash", parts, feeders, PROFILE]);
    assert!(
        o.status.success(),
        "{}{}",
        out(&o),
        String::from_utf8_lossy(&o.stderr)
    );
    let first = out(&o);
    let word = first
        .split_whitespace()
        .nth(1)
        .unwrap_or_else(|| panic!("{first}"))
        .to_string();
    assert!(is_fingerprint(&word), "{word}");
    word
}

#[test]
fn the_fingerprint_is_the_sha256_of_the_lines_each_ended_by_a_line_break() {
    // a value that was worked out apart from this program
    let lines = vec!["G28".to_string(), "M400".to_string()];
    assert_eq!(text(&lines), "G28\nM400\n");
    assert_eq!(
        fingerprint(&lines),
        "5abe332e7beb83726a894b78fbefebe1b9bf5a3f8172500aea4f854266c36679"
    );
    // either case is accepted, anything else is not a fingerprint
    assert!(matches(&lines, &fingerprint(&lines).to_ascii_uppercase()));
    assert!(matches(&lines, &format!("  {}\n", fingerprint(&lines))));
    for bad in ["", "abc", &"g".repeat(64), &"a".repeat(63), &"a".repeat(65)] {
        assert!(!is_fingerprint(bad), "{bad}");
        assert!(!matches(&lines, bad), "{bad}");
    }
    // the order of the lines and a line more or less are other fingerprints
    let swapped = vec!["M400".to_string(), "G28".to_string()];
    assert_ne!(fingerprint(&swapped), fingerprint(&lines));
    assert_ne!(fingerprint(&lines[..1]), fingerprint(&lines));
}

#[test]
fn the_command_prints_the_fingerprint_of_the_lines_that_script_out_writes() {
    let dir = temp("file");
    let script = dir.join("job.gcode");
    let o = run(&[
        "toolpath-hash",
        "examples/board.pos",
        "examples/feeders.csv",
        PROFILE,
        "--script-out",
        p(&script),
    ]);
    assert!(o.status.success(), "{}", out(&o));
    let written = std::fs::read(&script).unwrap();
    let want = eh3d_phoenix_pnp::sha256::sha256_hex(&written);
    assert!(out(&o).contains(&want), "{} / {want}", out(&o));
    // the file is not written over
    let again = run(&[
        "toolpath-hash",
        "examples/board.pos",
        "examples/feeders.csv",
        PROFILE,
        "--script-out",
        p(&script),
    ]);
    assert_eq!(again.status.code(), Some(1));
    // and it says that nothing was sent
    assert!(out(&o).contains("evidence=simulator"), "{}", out(&o));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn expect_ends_with_2_when_the_lines_are_not_the_authorised_ones() {
    let args = [
        "toolpath-hash",
        "examples/board.pos",
        "examples/feeders.csv",
        PROFILE,
    ];
    let good = hash_of("examples/board.pos", "examples/feeders.csv");
    let with = |e: &str, extra: &[&str]| {
        let mut a = args.to_vec();
        a.extend(["--expect", e]);
        a.extend(extra);
        run(&a)
    };
    assert_eq!(with(&good, &[]).status.code(), Some(0));
    assert_eq!(with(&good.to_ascii_uppercase(), &[]).status.code(), Some(0));
    let other = "0".repeat(64);
    let o = with(&other, &["--lang", "pt"]);
    assert_eq!(o.status.code(), Some(2), "{}", out(&o));
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("não são as que foram autorizadas"),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    // a text that is not a fingerprint is a mistake of the person, not a failed check
    assert_eq!(with("abc", &[]).status.code(), Some(1));
    assert_eq!(run(&args[..3]).status.code(), Some(1));
}

#[test]
fn a_place_that_moves_by_a_thousandth_of_a_millimetre_gives_another_fingerprint() {
    let dir = temp("move");
    let board = std::fs::read_to_string(root().join("examples/board.pos")).unwrap();
    assert!(board.contains("10.000     20.000"));
    let moved = board.replace("10.000     20.000", "10.001     20.000");
    let file = dir.join("moved.pos");
    std::fs::write(&file, moved).unwrap();
    let before = hash_of("examples/board.pos", "examples/feeders.csv");
    let after = hash_of(p(&file), "examples/feeders.csv");
    assert_ne!(before, after);
    // the same board gives the same fingerprint however often it is asked
    assert_eq!(
        before,
        hash_of("examples/board.pos", "examples/feeders.csv")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn boards_from_a_fixed_seed_give_the_same_fingerprint_again_and_each_its_own() {
    let dir = temp("seeds");
    let mut seen = std::collections::BTreeSet::new();
    for seed in 1..=12u32 {
        let (pos, feed) = (
            dir.join(format!("p{seed}.pos")),
            dir.join(format!("f{seed}.csv")),
        );
        let made = run(&[
            "test-board-gen",
            "mixed",
            p(&pos),
            p(&feed),
            "--seed",
            &seed.to_string(),
            "--count",
            "30",
        ]);
        assert!(made.status.success(), "{}", out(&made));
        let first = hash_of(p(&pos), p(&feed));
        assert_eq!(first, hash_of(p(&pos), p(&feed)), "seed {seed}");
        assert!(seen.insert(first), "seed {seed} repeats a fingerprint");
    }
    assert_eq!(seen.len(), 12);
    let _ = std::fs::remove_dir_all(&dir);
}
