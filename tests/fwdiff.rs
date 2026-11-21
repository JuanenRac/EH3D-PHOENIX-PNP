//! What two dumps of the configuration of a firmware give differently: the lines of G-code are read as values by letter, `80` and `80.00` are the same, the part a line is for is in the key, and
//! a change of firmware brings the warning to reset the EEPROM. Garbage from a fixed seed never breaks the reader, and the command ends with 2 when the values differ.

use eh3d_phoenix_pnp::fwdiff::{diff, parse, MAX_DUMP_BYTES};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const OLD: &str = include_str!("../examples/firmware_old.txt");
const NEW: &str = include_str!("../examples/firmware_new.txt");

fn dump(text: &str) -> eh3d_phoenix_pnp::fwdiff::Dump {
    parse(text).unwrap()
}

#[test]
fn a_dump_is_read_as_values_by_letter_with_the_part_in_the_key() {
    let d = dump(OLD);
    assert_eq!(
        d.firmware.as_deref(),
        Some("Marlin 2.1.2 (Github)"),
        "{:?}",
        d.firmware
    );
    let get = |k: &str| d.values.get(k).map(String::as_str);
    assert_eq!(get("M92 X"), Some("80.00"));
    assert_eq!(get("M92 E"), Some("500.00"));
    assert_eq!(get("M203 Z"), Some("10.00"));
    // the current of the motor 1 is not the current of the first, and the letter I of M301 would be a value
    assert_eq!(get("M906 Z"), Some("600"));
    assert_eq!(get("M906 I1 Z"), Some("600"));
    assert_eq!(get("M914 X"), Some("80"));
    // a comment, an answer and the units line add nothing
    assert!(
        d.values.keys().all(|k| k.starts_with('M')),
        "{:?}",
        d.values
    );
    assert!(!d.values.contains_key("G21 "));
    // the preset of a heater is named by its S
    let h = dump("echo:  M145 S0 H180 B70 F0\necho:  M145 S1 H240 B110 F1\n");
    assert_eq!(h.values.get("M145 S0 H").map(String::as_str), Some("180"));
    assert_eq!(h.values.get("M145 S1 H").map(String::as_str), Some("240"));
    let pid = dump("M301 P22.2 I1.08 D114\n");
    assert_eq!(pid.values.get("M301 I").map(String::as_str), Some("1.08"));
}

#[test]
fn a_firmware_is_named_by_either_line_and_a_dump_without_values_is_not_one() {
    let a = dump("echo:Marlin 2.0.9.3\nM92 X80\n");
    assert_eq!(a.firmware.as_deref(), Some("Marlin 2.0.9.3"));
    assert!(dump("M92 X80\n").firmware.is_none());
    for bad in ["", "ok\n", "echo:; only comments\n", "garbage words\n"] {
        assert!(parse(bad).is_err(), "{bad:?}");
    }
    assert!(parse(&"M92 X1\n".repeat(MAX_DUMP_BYTES / 4)).is_err());
}

#[test]
fn values_written_differently_that_mean_the_same_are_not_differences() {
    let d = diff(&dump(OLD), &dump(OLD));
    assert!(d.is_same() && d.same_meaning == 0);
    let a = dump("M92 X80.00 Y80\nM145 S0 H180\n");
    let b = dump("m92 x80 y80.000\nM145 S0 H180.0\n");
    let d = diff(&a, &b);
    assert!(d.is_same(), "{:?}", d.changes);
    assert_eq!(d.same_meaning, 3);
}

#[test]
fn the_changes_are_the_ones_of_the_two_examples() {
    let d = diff(&dump(OLD), &dump(NEW));
    assert_eq!(d.same_meaning, 1, "M92 X is 80.00 and 80");
    assert_eq!(d.counts(), (1, 3, 4));
    assert_eq!(
        d.lines(),
        [
            "M203 X: 300.00 -> 350.00",
            "M204 T: 3000.00 -> 2500.00",
            "M206 X: 0.00 -> -",
            "M206 Y: 0.00 -> -",
            "M206 Z: 0.00 -> -",
            "M906 X: 800 -> 900",
            "M913 X: - -> 100",
            "M914 X: 80 -> 70",
        ]
    );
    assert_eq!(
        d.firmware_changed(),
        Some(("Marlin 2.1.2 (Github)", "Marlin 2.1.3 (Github)"))
    );
}

#[test]
fn the_same_firmware_or_a_dump_that_does_not_say_has_no_warning() {
    let a = dump("echo:Marlin 2.1.2\nM92 X80\n");
    assert!(diff(&a, &a).firmware_changed().is_none());
    let unnamed = dump("M92 X80\n");
    assert!(diff(&a, &unnamed).firmware_changed().is_none());
    // the case of the name does not make another firmware
    let b = dump("FIRMWARE_NAME:MARLIN 2.1.2\nM92 X80\n");
    assert!(diff(&dump("FIRMWARE_NAME:marlin 2.1.2\nM92 X80\n"), &b).is_same());
}

/// A generator of lines from a fixed seed.
fn noise(seed: u64, lines: usize) -> String {
    let mut s = seed;
    let mut next = move |n: u64| {
        s = s
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (s >> 33) % n
    };
    let words = [
        "M92",
        "M203",
        "M906",
        "M914",
        "G21",
        "echo:",
        "ok",
        ";",
        "X80",
        "Y-3.5",
        "I1",
        "S0",
        "H180",
        "T",
        "E",
        "\u{e9}",
        "FIRMWARE_NAME:",
        "Marlin",
        "2.1",
        "1e999",
        "",
        "M",
        "G.",
    ];
    let mut out = String::new();
    for _ in 0..lines {
        for _ in 0..next(6) {
            out.push_str(words[next(words.len() as u64) as usize]);
            out.push(' ');
        }
        out.push('\n');
    }
    out
}

#[test]
fn garbage_from_a_fixed_seed_never_breaks_the_reader() {
    let mut read = 0;
    for seed in 0..300u64 {
        let text = noise(seed, 40);
        if let Ok(d) = parse(&text) {
            read += 1;
            // a dump is always the same as itself and never has a change against itself
            assert!(diff(&d, &d).is_same(), "seed {seed}");
        }
    }
    assert!(read > 50, "only {read} of 300 dumps were read");
    // the same seed, the same dump
    assert_eq!(noise(7, 40), noise(7, 40));
}

// ---- the command

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

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_fw_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

const A: &str = "examples/firmware_old.txt";
const B: &str = "examples/firmware_new.txt";

#[test]
fn the_command_lists_the_differences_warns_of_the_eeprom_and_ends_with_2() {
    let o = run(&["fw-config-diff", A, B]);
    assert_eq!(o.status.code(), Some(2), "{}{}", out(&o), err(&o));
    let text = out(&o);
    assert!(text.contains("M203 X: 300.00 -> 350.00"), "{text}");
    assert!(text.contains("M913 X: - -> 100"), "{text}");
    assert!(
        text.contains("8 values differ (4 changed, 1 only in the second, 3 only in the first); 1 are written differently"),
        "{text}"
    );
    assert!(
        text.contains("the firmware changed from Marlin 2.1.2 (Github) to Marlin 2.1.3 (Github)")
            && text.contains("reset the EEPROM (M502, then M500)"),
        "{text}"
    );
    assert!(err(&o).contains("8 values of the firmware differ"));
    let o = run(&["fw-config-diff", A, B, "--lang", "pt"]);
    assert!(out(&o).contains("reponha a EEPROM"), "{}", out(&o));
    let o = run(&["fw-config-diff", A, B, "--lang", "es"]);
    assert!(out(&o).contains("restablece la EEPROM"), "{}", out(&o));
}

#[test]
fn the_same_dump_ends_with_0_and_a_changed_firmware_alone_is_a_warning() {
    let o = run(&["fw-config-diff", A, A]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    assert!(out(&o).contains("0 values differ"));
    assert!(!out(&o).contains("warning"));
    // the values are the same and the firmware is not: the EEPROM may still hold the old ones
    let dir = temp("warn");
    let renamed: String = OLD
        .lines()
        .map(|l| {
            if l.starts_with("FIRMWARE_NAME:") {
                "FIRMWARE_NAME:Marlin 2.2.0"
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let file = dir.join("renamed.txt");
    std::fs::write(&file, renamed).unwrap();
    let o = run(&["fw-config-diff", A, p(&file)]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    assert!(
        out(&o).contains("warning: the firmware changed"),
        "{}",
        out(&o)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_that_is_not_a_dump_or_a_wrong_argument_is_an_error_of_the_command() {
    let dir = temp("bad");
    let junk = dir.join("junk.txt");
    std::fs::write(&junk, "ok\nnothing here\n").unwrap();
    let o = run(&["fw-config-diff", A, p(&junk)]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("no line of G-code"), "{}", err(&o));
    assert_eq!(run(&["fw-config-diff", A]).status.code(), Some(1));
    assert_eq!(
        run(&["fw-config-diff", A, "no_such.txt"]).status.code(),
        Some(1)
    );
    assert_eq!(
        run(&["fw-config-diff", A, B, "--lang", "xx"]).status.code(),
        Some(1)
    );
    assert_eq!(run(&["fw-config-diff", A, B, "--x"]).status.code(), Some(1));
    let _ = std::fs::remove_dir_all(&dir);
}
