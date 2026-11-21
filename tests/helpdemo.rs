//! What the help teaches works. Every example the help shows (a line that begins `example: phoenix-pnp`) is run, from the folder of the repository, and must end well and print something; a
//! command that the help lists is known to the program; and the exit codes the help promises are the ones the program gives.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_phoenix-pnp");

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn help() -> String {
    let out = Command::new(BIN).arg("--help").output().unwrap();
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn examples() -> Vec<Vec<String>> {
    help()
        .lines()
        .filter_map(|l| l.trim().strip_prefix("example: phoenix-pnp "))
        .map(|rest| rest.split_whitespace().map(str::to_string).collect())
        .collect()
}

#[test]
fn every_example_of_the_help_runs_and_prints_something() {
    let list = examples();
    assert!(list.len() >= 6, "the help teaches {} examples", list.len());
    for args in &list {
        let out = Command::new(BIN)
            .args(args)
            .current_dir(root())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?} ended with {:?}: {}{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!out.stdout.is_empty(), "{args:?} printed nothing");
    }
}

#[test]
fn the_examples_of_the_help_name_files_that_are_there_and_write_nothing() {
    for args in examples() {
        for a in &args {
            if a.contains('/')
                || (a.contains('.') && !a.starts_with("--") && a.parse::<f64>().is_err())
            {
                assert!(
                    root().join(a).is_file(),
                    "{args:?}: {a} is not a file of the repository"
                );
            }
            assert_ne!(a, "--out", "{args:?}: an example writes no file");
        }
    }
}

#[test]
fn the_help_says_its_exit_codes_and_the_program_gives_them() {
    let h = help();
    assert!(h.contains("Exit codes: 0 when all is well, 1 when"), "{h}");
    assert!(h.contains("2 when a check of the command is not passed"));
    // 0: an example; 1: a file that is not there; 2: a check that is not passed (a link that never answers)
    let code = |args: &[&str]| {
        Command::new(BIN)
            .args(args)
            .current_dir(root())
            .output()
            .unwrap()
            .status
            .code()
            .unwrap()
    };
    assert_eq!(code(&["parts", "examples/board.pos"]), 0);
    assert_eq!(code(&["parts", "examples/no_such_file.pos"]), 1);
    assert_eq!(code(&["node-sim", "--steps", "5", "--silence", "1000"]), 2);
}

#[test]
fn every_command_the_help_lists_is_known_to_the_program() {
    let mut commands: Vec<String> = help()
        .lines()
        .filter_map(|l| l.strip_prefix("  phoenix-pnp "))
        .filter_map(|rest| rest.split_whitespace().next())
        .filter(|w| !w.starts_with('[') && !w.starts_with('-'))
        .map(str::to_string)
        .collect();
    commands.sort();
    commands.dedup();
    assert!(commands.len() > 50, "{} commands", commands.len());
    for c in commands {
        // the window opens on its own, not here
        if c == "gui" {
            continue;
        }
        let out = Command::new(BIN)
            .arg(&c)
            .current_dir(root())
            .output()
            .unwrap();
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(!said.contains("unknown command"), "{c}: {said}");
    }
}
