//! What the README and the script run.sh promise can be run: they show the same sample, the command works on the files of examples/ (its output goes to a temporary folder) and the help runs.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_phoenix-pnp");

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(name: &str) -> String {
    std::fs::read_to_string(root().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_string).collect()
}

fn of_readme() -> Vec<Vec<String>> {
    read("README.md")
        .lines()
        .filter_map(|l| l.trim().strip_prefix("cargo run --release -- ").map(words))
        .collect()
}

fn of_run_sh() -> Vec<String> {
    let text = read("run.sh");
    let line = text
        .lines()
        .find(|l| l.contains("set -- ") && !l.contains("\"$@\""))
        .expect("run.sh has the sample");
    words(
        line.split("set -- ")
            .nth(1)
            .unwrap()
            .split(';')
            .next()
            .unwrap(),
    )
}

#[test]
fn the_script_and_the_readme_show_the_same_sample() {
    let sample = of_run_sh();
    assert!(!sample.is_empty());
    assert!(
        of_readme().contains(&sample),
        "the README does not show the sample of run.sh"
    );
}

#[test]
fn the_commands_of_the_readme_run() {
    let commands = of_readme();
    assert!(!commands.is_empty(), "the README shows no command");
    for (i, args) in commands.iter().enumerate() {
        let dir =
            std::env::temp_dir().join(format!("phoenix_pnp_readme_{i}_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // a file of examples/ that does not exist yet is an output: it goes to the temporary folder
        let args: Vec<String> = args
            .iter()
            .map(|a| {
                if a.starts_with("examples/") && !root().join(a).exists() {
                    dir.join(a.replace('/', "_")).to_string_lossy().into_owned()
                } else {
                    a.clone()
                }
            })
            .collect();
        let out = Command::new(BIN)
            .args(&args)
            .current_dir(root())
            .output()
            .expect("the program runs");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!out.stdout.is_empty(), "{args:?} printed nothing");
    }
}

#[test]
fn the_help_runs_and_names_the_commands() {
    let out = Command::new(BIN).arg("help").output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    for command in ["parts", "check", "plan", "--origin-x"] {
        assert!(text.contains(command), "the help does not name {command}");
    }
}

fn readmes() -> Vec<(String, String)> {
    [
        "README.md",
        "README_spa.md",
        "README_deu.md",
        "README_fra.md",
        "README_ita.md",
        "README_jpn.md",
        "README_zho.md",
        "README_por.md",
    ]
    .iter()
    .map(|name| (name.to_string(), read(name)))
    .collect()
}

#[test]
fn every_error_code_is_documented_in_the_eight_readmes() {
    for (name, text) in readmes() {
        for code in eh3d_phoenix_pnp::error::Code::ALL {
            assert!(
                text.contains(&code.label()),
                "{name} does not document {}",
                code.label()
            );
        }
    }
}

#[test]
fn the_readmes_count_the_tests_that_exist() {
    // the marker is built here so that this file does not count itself twice
    let marker = format!("#[{}]", "test");
    let mut total = 0;
    for dir in ["src", "tests"] {
        for entry in std::fs::read_dir(root().join(dir)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "rs") {
                total += std::fs::read_to_string(&path)
                    .unwrap()
                    .lines()
                    .filter(|l| l.contains(&marker))
                    .count();
            }
        }
    }
    for (name, text) in readmes() {
        assert!(
            text.contains(&format!("| {total} |")),
            "{name} does not say {total} tests"
        );
    }
    assert!(read("README.md").contains(&format!("{total} tests")));
}
