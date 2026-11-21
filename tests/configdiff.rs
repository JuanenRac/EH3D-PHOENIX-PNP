//! What two configuration files give differently, in what they mean: the same value written another way, the default, keys that are listed more than once, damaged files and `config-diff`.

use eh3d_phoenix_pnp::config::MAX_CONFIG_BYTES;
use eh3d_phoenix_pnp::configdiff::diff;
use eh3d_phoenix_pnp::error::Code;
use std::path::PathBuf;
use std::process::{Command, Output};

const MACHINE: &str = include_str!("../examples/machine.cfg");

fn keys(a: &str, b: &str) -> Vec<String> {
    diff(a, b)
        .unwrap()
        .changes
        .into_iter()
        .map(|c| c.key)
        .collect()
}

#[test]
fn a_file_is_the_same_as_itself_and_the_example_differs_from_nothing_only_where_it_is_not_the_default(
) {
    let d = diff(MACHINE, MACHINE).unwrap();
    assert!(d.is_same());
    assert_eq!(d.same_meaning, 0);
    assert_eq!(
        d.text(),
        "0 settings differ, 0 are written differently and mean the same\n"
    );
    // the example writes out the defaults and also sets a few things that are not the default: those are what it differs from nothing in
    let against_empty = diff("", MACHINE).unwrap();
    let names: Vec<&str> = against_empty
        .changes
        .iter()
        .map(|c| c.key.as_str())
        .collect();
    assert!(names.contains(&"nozzles") && names.contains(&"rotation") && names.contains(&"head"));
    assert!(!names.contains(&"speed_mm_s") && !names.contains(&"safe_z_mm"));
    assert!(
        against_empty.same_meaning > 50,
        "{}",
        against_empty.same_meaning
    );
}

#[test]
fn a_value_that_changes_is_told_with_the_two_values() {
    let d = diff("nozzles = 2\n", "nozzles = 4\nspeed_mm_s = 300\n").unwrap();
    let shown: Vec<(&str, Vec<String>, Vec<String>)> = d
        .changes
        .iter()
        .map(|c| (c.key.as_str(), c.old.clone(), c.new.clone()))
        .collect();
    assert_eq!(
        shown,
        vec![
            ("nozzles", vec!["2".to_string()], vec!["4".to_string()]),
            ("speed_mm_s", vec![], vec!["300".to_string()]),
        ]
    );
    assert_eq!(
        d.text(),
        "nozzles: 2 -> 4\nspeed_mm_s: (default) -> 300\n2 settings differ, 0 are written differently and mean the same\n"
    );
}

#[test]
fn the_same_meaning_written_another_way_is_not_a_difference() {
    // yes and on, 5 and 5.0, the case of the key, the spaces and a comment
    let a = "pre_rotate = yes\nsafe_z_mm = 5\nnozzles = 1\n";
    let b = "# a comment\n  PRE_ROTATE   =   on  \nsafe_z_mm = 5.0\n";
    let d = diff(a, b).unwrap();
    assert!(d.is_same(), "{:?}", d.changes);
    // nozzles = 1 is the default: a file that says it and one that does not are the same machine
    assert_eq!(
        d.same_meaning, 3,
        "pre_rotate, safe_z_mm and the line that gives the default are written differently"
    );
}

#[test]
fn a_line_that_gives_the_default_is_the_same_as_no_line_but_not_another_value() {
    assert!(keys("nozzles = 1\n", "").is_empty());
    assert!(keys("", "nozzles = 1\n").is_empty());
    assert_eq!(keys("", "nozzles = 3\n"), vec!["nozzles"]);
    assert_eq!(keys("nozzles = 3\n", ""), vec!["nozzles"]);
}

#[test]
fn a_key_that_may_be_written_more_than_once_is_compared_as_the_list_it_is() {
    let a = "sim_part_fail = R_0603 : 0.1 : 0\nsim_part_fail = C_0603 : 0.2 : 0\n";
    let b = "sim_part_fail = R_0603 : 0.1 : 0\n";
    let d = diff(a, b).unwrap();
    assert_eq!(d.changes.len(), 1);
    assert_eq!(d.changes[0].key, "sim_part_fail");
    assert_eq!(d.changes[0].old.len(), 2);
    assert_eq!(d.changes[0].new, vec!["R_0603 : 0.1 : 0".to_string()]);
    assert!(d
        .text()
        .contains("sim_part_fail: R_0603 : 0.1 : 0 ; C_0603 : 0.2 : 0 -> R_0603 : 0.1 : 0\n"));
}

#[test]
fn a_file_that_is_not_valid_is_the_error_of_that_file() {
    let (which, e) = diff("nozzles = 2\n", "nozzles = 99\n").unwrap_err();
    assert_eq!(which, "second");
    assert_eq!(e.code, Code::ConfigOutOfRange);
    let (which, e) = diff("no_such_key = 1\n", "").unwrap_err();
    assert_eq!(which, "first");
    assert_eq!(e.code, Code::ConfigUnknownKey);
    let (which, _) = diff("nozzles = 2\nnozzles = 3\n", "").unwrap_err();
    assert_eq!(which, "first");
    let big = "# ".repeat(MAX_CONFIG_BYTES);
    assert_eq!(diff(&big, "").unwrap_err().1.code, Code::FileTooBig);
}

#[test]
fn a_byte_order_mark_and_windows_line_ends_change_nothing() {
    let a = "\u{feff}nozzles = 3\r\nspeed_mm_s = 200\r\n";
    let b = "speed_mm_s = 200\nnozzles = 3\n";
    let d = diff(a, b).unwrap();
    assert!(d.is_same());
    assert_eq!(d.same_meaning, 0);
}

// ---- config-diff

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

#[test]
fn the_command_says_what_differs_and_exits_with_2() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_configdiff_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let a = dir.join("a.cfg");
    let b = dir.join("b.cfg");
    let c = dir.join("c.cfg");
    std::fs::write(&a, "nozzles = 1\npre_rotate = yes\n").unwrap();
    std::fs::write(&b, "nozzles = 3\npre_rotate = on\n").unwrap();
    std::fs::write(&c, "nozzles = 1\npre_rotate = on\n").unwrap();
    let (pa, pb, pc) = (
        a.to_str().unwrap(),
        b.to_str().unwrap(),
        c.to_str().unwrap(),
    );
    let o = run(&["config-diff", pa, pb]);
    assert!(!o.status.success());
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(out.contains("nozzles: 1 -> 3\n"), "{out}");
    assert!(!out.contains("pre_rotate"), "{out}");
    assert!(String::from_utf8_lossy(&o.stderr).contains("1 settings differ"));
    let o = run(&["config-diff", pa, pc]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(String::from_utf8_lossy(&o.stdout).contains("1 are written differently"));
    // an invalid file is named with its path, in the language asked for
    std::fs::write(&c, "nozzles = 99\n").unwrap();
    let o = run(&["config-diff", pa, pc, "--lang", "es"]);
    assert!(!o.status.success());
    let err = String::from_utf8_lossy(&o.stderr).into_owned();
    assert!(
        err.contains("c.cfg") && err.contains("PHOENIX-PNP-006"),
        "{err}"
    );
    // bad words
    assert!(!run(&["config-diff", pa]).status.success());
    assert!(!run(&["config-diff", pa, pb, "--lang", "xx"])
        .status
        .success());
    assert!(!run(&["config-diff", pa, pb, "--x"]).status.success());
    assert!(!run(&["config-diff", pa, "no_such_file.cfg"])
        .status
        .success());
    let _ = std::fs::remove_dir_all(&dir);
}
