//! The configuration of a Klipper controller is compared as the answer of a Marlin one is: a value is `section key`, `40` and `40.0` are the same, what `SAVE_CONFIG` saved wins over the file
//! above it, a macro is one value, and a Klipper file is never compared with a Marlin dump. Garbage from a fixed seed never breaks the reader.

use eh3d_phoenix_pnp::fwdiff::{diff, looks_like_klipper, parse};
use std::path::PathBuf;
use std::process::{Command, Output};

const OLD: &str = include_str!("../examples/klipper_old.cfg");
const NEW: &str = include_str!("../examples/klipper_new.cfg");
const MARLIN: &str = include_str!("../examples/firmware_old.txt");

/// The text with the line ends of a clone made on Windows or on Linux brought to one form.
fn lf(text: &str) -> String {
    text.replace("\r\n", "\n")
}

#[test]
fn a_klipper_file_is_told_by_its_sections_and_a_marlin_dump_is_not() {
    assert!(looks_like_klipper(&lf(OLD)));
    assert!(!looks_like_klipper(&lf(MARLIN)));
    // the lines of G-code inside a macro do not make it a Marlin dump
    assert!(parse(&lf(OLD)).unwrap().klipper);
    assert!(!parse(&lf(MARLIN)).unwrap().klipper);
    assert!(parse("[just_a_section]\n").is_err());
}

#[test]
fn a_value_is_its_section_and_its_key_and_the_saved_block_wins() {
    let d = parse(&lf(OLD)).unwrap();
    let get = |k: &str| d.values.get(k).map(String::as_str);
    assert_eq!(get("stepper_x rotation_distance"), Some("40"));
    assert_eq!(get("tmc2209 stepper_x driver_sgthrs"), Some("80"));
    // a comment after a blank is not part of the value, and a pin that begins with ^ is
    assert_eq!(get("stepper_y endstop_pin"), Some("^PG6"));
    // the saved block replaces what the file said above it, and a key that only it has is added
    assert_eq!(get("stepper_y position_endstop"), Some("0.5"));
    // a macro is one value, with its lines joined
    assert_eq!(
        get("gcode_macro NOZZLE_UP gcode"),
        Some("M42 P5 S0; G4 P200")
    );
    // the comment lines and the warning of the saved block add nothing
    assert!(d.values.keys().all(|k| !k.contains('#')), "{:?}", d.values);
}

#[test]
fn the_changes_are_the_ones_of_the_two_examples() {
    let (a, b) = (parse(&lf(OLD)).unwrap(), parse(&lf(NEW)).unwrap());
    let d = diff(&a, &b);
    assert!(d.klipper);
    // 40 and 40.0 mean the same and are not a change
    assert_eq!(d.same_meaning, 1);
    let lines = d.lines();
    for want in [
        "stepper_x microsteps: 16 -> 32",
        "stepper_x homing_speed: 40 -> 60",
        "tmc2209 stepper_x driver_sgthrs: 80 -> 110",
        "printer max_accel: 3000 -> 5000",
        "gcode_macro NOZZLE_UP gcode: M42 P5 S0; G4 P200 -> M42 P5 S0; G4 P300",
        "gcode_macro NOZZLE_DOWN gcode: - -> M42 P5 S1",
        "stepper_y position_endstop: 0.5 -> 0.75",
    ] {
        assert!(lines.iter().any(|l| l == want), "{want}: {lines:?}");
    }
    assert_eq!(d.changes.len(), 7, "{lines:?}");
    // the file is the same as itself
    assert!(diff(&a, &a).is_same());
    // no firmware is named in the file, so nothing says the EEPROM is to be reset
    assert!(d.firmware_changed().is_none());
}

/// A text from a seed: lines of a Klipper file, some of them cut or out of place.
fn noise(seed: u64, lines: usize) -> String {
    let mut s = seed ^ 0x9e37_79b9_7f4a_7c15;
    let mut next = |n: u64| {
        s = s
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (s >> 33) % n
    };
    let pieces = [
        "[stepper_x]",
        "[printer]",
        "[ ]",
        "[",
        "key: value",
        "rotation_distance = 40.00",
        ":",
        "= 3",
        "  continued line",
        "# a comment",
        "; another",
        "#*# <---- SAVE_CONFIG ---->",
        "#*# [stepper_y]",
        "#*# position_endstop = 1.5",
        "gcode:",
        "    G28",
        "\u{feff}",
        "é: ü",
    ];
    (0..lines)
        .map(|_| pieces[next(pieces.len() as u64) as usize])
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn garbage_from_a_fixed_seed_never_breaks_the_reader() {
    let mut read = 0;
    for seed in 0..400u64 {
        let text = noise(seed, 30);
        if let Ok(d) = parse(&text) {
            read += 1;
            assert!(diff(&d, &d).is_same(), "seed {seed}");
        }
    }
    assert!(read > 50, "only {read} of 400 files were read");
    assert_eq!(noise(11, 30), noise(11, 30));
}

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

#[test]
fn the_command_compares_two_klipper_files_notes_the_saved_block_and_ends_with_2() {
    let (a, b) = ("examples/klipper_old.cfg", "examples/klipper_new.cfg");
    let o = run(&["fw-config-diff", a, b]);
    assert_eq!(o.status.code(), Some(2), "{}", out(&o));
    let text = out(&o);
    assert!(text.contains("stepper_x microsteps: 16 -> 32"), "{text}");
    assert!(
        text.contains("7 values differ (6 changed, 1 only in the second, 0 only in the first); 1 are written differently"),
        "{text}"
    );
    assert!(text.contains("SAVE_CONFIG"), "{text}");
    // the note is in the language that was asked for
    let o = run(&["fw-config-diff", a, b, "--lang", "pt"]);
    assert!(
        out(&o).contains("mandam sobre o mesmo valor"),
        "{}",
        out(&o)
    );
    // the same file ends well
    assert_eq!(run(&["fw-config-diff", a, a]).status.code(), Some(0));
}

#[test]
fn a_klipper_file_and_a_marlin_dump_are_not_compared() {
    let o = run(&[
        "fw-config-diff",
        "examples/klipper_old.cfg",
        "examples/firmware_old.txt",
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("cannot be compared"));
}
