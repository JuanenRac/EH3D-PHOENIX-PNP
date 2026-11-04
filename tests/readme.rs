//! What the README and the script run.sh promise can be run: they show the same sample, the command works on the files of examples/ (its output goes to a temporary folder) and the help runs.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_eh3d-pnp");

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
        let dir = std::env::temp_dir().join(format!("eh3d_pnp_readme_{i}_{}", std::process::id()));
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
