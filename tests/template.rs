//! The starting points for the settings of a machine: every template is a valid file, says why it is there, differs from the defaults in what it claims, and `machine-template` lists and writes
//! them.

use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::server::Mode;
use eh3d_phoenix_pnp::template::{all, find, list_text};
use std::path::PathBuf;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_phoenix-pnp");

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "phoenix_pnp_template_{name}_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn every_template_is_a_valid_file_of_settings() {
    assert_eq!(all().len(), 7);
    for t in all() {
        let text = t.text();
        let cfg = MachineConfig::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", t.name));
        cfg.validate().unwrap_or_else(|e| panic!("{}: {e}", t.name));
        assert!(!t.about.is_empty() && !t.name.contains(' '));
    }
}

#[test]
fn every_key_a_template_sets_is_a_key_of_the_settings_and_is_set_once() {
    for t in all().iter().filter(|t| t.name != "full") {
        let mut seen = Vec::new();
        let text = t.text();
        for line in text
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        {
            let key = line.split('=').next().unwrap().trim();
            assert!(KEYS.contains(&key), "{}: {key}", t.name);
            if !["head"].contains(&key) {
                assert!(!seen.contains(&key), "{}: {key} twice", t.name);
            }
            seen.push(key);
        }
        assert!(!seen.is_empty());
        // it starts with what it is
        assert!(t.text().starts_with(&format!("# {}: {}.", t.name, t.about)));
    }
}

#[test]
fn each_template_changes_what_it_says_it_changes() {
    let default = MachineConfig::default();
    let get = |name: &str| MachineConfig::parse(&find(name).unwrap().text()).unwrap();
    assert_eq!(get("desktop-1").nozzles, 1);
    assert_eq!(get("desktop-2").nozzles, 2);
    let dual = get("dual-head");
    assert_eq!(dual.heads.len(), 2);
    assert_eq!(
        (dual.heads[0].name.as_str(), dual.heads[1].offset_x_mm),
        ("left", 60.0)
    );
    assert_eq!(get("production-4").nozzles, 4);
    assert!(get("production-4").x_speed_mm_s > get("desktop-1").x_speed_mm_s);
    let sim = get("simulation");
    assert!(
        sim.sim_feed_fail_rate > 0.0
            && sim.sim_pick_fail_rate > 0.0
            && sim.sim_place_fail_rate > 0.0
    );
    let server = get("server");
    assert_eq!(server.mode, Mode::Server);
    assert_eq!(server.server_machine, "pnp-1");
    assert_eq!(
        server.server_token, "",
        "the token is a comment: it is not in a file that is copied"
    );
    assert!(MachineConfig::parse(&find("server").unwrap().text()).is_ok());
    // a template that says nothing about the server leaves the local mode
    assert_eq!(get("desktop-1").mode, default.mode);
}

#[test]
fn the_full_template_is_the_documented_example_as_it_is() {
    let full = find("full").unwrap().text();
    assert_eq!(full, include_str!("../examples/machine.cfg"));
    for key in KEYS {
        assert!(
            full.contains(&format!("\n{key} =")) || full.starts_with(&format!("{key} =")),
            "{key}"
        );
    }
}

#[test]
fn a_name_is_found_without_regard_to_case_and_a_wrong_one_is_not() {
    assert_eq!(find(" Desktop-1 ").unwrap().name, "desktop-1");
    assert!(find("desktop").is_none() && find("").is_none());
    let list = list_text();
    assert_eq!(list.lines().count(), all().len());
    for t in all() {
        assert!(list.contains(&format!("{}: {}", t.name, t.about)));
    }
}

#[test]
fn the_command_lists_prints_and_writes_and_never_over_a_file() {
    let o = run(&["machine-template"]);
    assert!(o.status.success());
    assert_eq!(out(&o), list_text());
    let o = run(&["machine-template", "production-4"]);
    assert!(o.status.success());
    assert_eq!(out(&o), find("production-4").unwrap().text());
    let dir = temp("write");
    let file = dir.join("machine.cfg").to_string_lossy().into_owned();
    let o = run(&["machine-template", "desktop-2", "--out", &file]);
    assert!(o.status.success(), "{}", err(&o));
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        find("desktop-2").unwrap().text()
    );
    // the file written is a file that the other commands take as settings
    let check = run(&["config-check", &file]);
    assert!(check.status.success(), "{}", err(&check));
    let again = run(&["machine-template", "desktop-2", "--out", &file]);
    assert_eq!(again.status.code(), Some(1));
    assert!(err(&again).contains("PHOENIX-PNP-040"), "{}", err(&again));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_wrong_name_or_a_word_too_many_is_an_error_that_names_the_choices() {
    let o = run(&["machine-template", "nope"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        err(&o).contains("no such template") && err(&o).contains("desktop-1"),
        "{}",
        err(&o)
    );
    let o = run(&["machine-template", "a", "b"]);
    assert_eq!(o.status.code(), Some(1));
    let o = run(&["machine-template", "--nope"]);
    assert_eq!(o.status.code(), Some(1));
}
