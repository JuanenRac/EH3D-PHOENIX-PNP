//! Every command of the program, given nothing, a wrong option and files that are not there, answers with an error line and the exit code 1 (or with success when it needs nothing), and never
//! panics; the help names every command that the program has and every command of the help is one that the program knows.

use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_phoenix-pnp");
const MAIN: &str = include_str!("../src/main.rs");

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// The commands that the help names: the word after `phoenix-pnp` at the start of a line (`gui` opens a window and is left out).
fn help_commands() -> Vec<String> {
    let o = run(&["help"]);
    assert!(o.status.success());
    let mut names: Vec<String> = text(&o.stdout)
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix("phoenix-pnp "))
        .filter_map(|l| l.split_whitespace().next())
        .filter(|w| w.chars().all(|c| c.is_ascii_lowercase() || c == '-') && !w.is_empty())
        .map(str::to_string)
        .collect();
    names.sort();
    names.dedup();
    names.retain(|n| n != "gui");
    names
}

/// The commands that `main.rs` runs: the text before `=>` in the match of the commands.
fn dispatched_commands() -> Vec<String> {
    let mut names: Vec<String> = MAIN
        .lines()
        .filter_map(|l| {
            let t = l.trim();
            let (name, rest) = t.strip_prefix('"')?.split_once("\" =>")?;
            let call = rest.trim_start();
            (call.starts_with("commands::") || call.starts_with("cmd_")).then(|| name.to_string())
        })
        .filter(|n| n.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
        .collect();
    names.sort();
    names.dedup();
    names
}

fn no_panic(o: &Output, what: &str) {
    let all = format!("{}{}", text(&o.stdout), text(&o.stderr));
    assert!(!all.contains("panicked"), "{what}: {all}");
    assert!(
        matches!(o.status.code(), Some(0 | 1)),
        "{what}: exit {:?}",
        o.status.code()
    );
}

#[test]
fn the_help_names_a_lot_of_commands_and_every_one_of_them_is_a_command_of_the_program() {
    let names = help_commands();
    assert!(names.len() >= 60, "{names:?}");
    for n in &names {
        let o = run(&[n]);
        assert!(
            !text(&o.stderr).contains("unknown command"),
            "{n} is in the help and is not a command"
        );
    }
}

#[test]
fn every_command_of_the_program_is_in_the_help() {
    let help = help_commands();
    let dispatched = dispatched_commands();
    assert!(dispatched.len() >= 60, "{dispatched:?}");
    for n in &dispatched {
        if ["help", "gui", "parts", "check", "plan"].contains(&n.as_str()) {
            continue;
        }
        assert!(
            help.contains(n),
            "{n} is a command of the program and the help does not name it"
        );
    }
    // and the other way
    for n in &help {
        assert!(
            dispatched.contains(n) || ["parts", "check", "plan", "help"].contains(&n.as_str()),
            "{n} is in the help and not in main.rs"
        );
    }
}

#[test]
fn a_command_given_nothing_says_what_it_needs_or_does_its_work_and_never_panics() {
    for n in help_commands() {
        let o = run(&[&n]);
        no_panic(&o, &n);
        if o.status.code() == Some(1) {
            assert!(
                text(&o.stderr).starts_with("error:"),
                "{n}: {}",
                text(&o.stderr)
            );
        }
    }
}

#[test]
fn a_command_given_an_option_it_does_not_have_refuses_it() {
    for n in help_commands() {
        let o = run(&[&n, "--no-such-option-at-all"]);
        no_panic(&o, &n);
        assert_eq!(
            o.status.code(),
            Some(1),
            "{n} takes --no-such-option-at-all"
        );
        assert!(
            text(&o.stderr).starts_with("error:"),
            "{n}: {}",
            text(&o.stderr)
        );
    }
}

#[test]
fn a_command_given_files_that_are_not_there_says_so_and_never_panics() {
    // files in a folder that does not exist, so that a command that would write one cannot
    let gone =
        std::env::temp_dir().join(format!("phoenix_pnp_cli_all_{}_gone", std::process::id()));
    let file = |n: u8| -> String {
        gone.join(format!("not_there_{n}.x"))
            .to_string_lossy()
            .into_owned()
    };
    // the queue is a file that may be missing: the queue that is not there has nothing to run
    let empty_is_fine = ["queue-run", "queue-status"];
    let files = [file(1), file(2)];
    for n in help_commands() {
        let mut args = vec![n.as_str()];
        args.extend(files.iter().map(String::as_str));
        let o = run(&args);
        no_panic(&o, &format!("{args:?}"));
        if !empty_is_fine.contains(&n.as_str()) {
            assert_eq!(
                o.status.code(),
                Some(1),
                "{args:?}: {}{}",
                text(&o.stdout),
                text(&o.stderr)
            );
        }
    }
    assert!(!gone.exists());
}

#[test]
fn an_unknown_command_is_an_error_that_names_it() {
    let o = run(&["no-such-command"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        text(&o.stderr).contains("no-such-command: unknown command"),
        "{}",
        text(&o.stderr)
    );
}
