//! Homing without end switches in the profile of the driver: the mode, a sensitivity for each of X and Y, the line that sets it and the order of the lines that go to the controller. All of it
//! is tried against the simulated controller with a fixed seed; no motor has felt a stop.

use eh3d_phoenix_pnp::driver::Session;
use eh3d_phoenix_pnp::gcodedriver::{DriverProfile, Fault, HomingMode};
use eh3d_phoenix_pnp::gcodesim::{Faults, GcodeSim, SimLink};
use std::path::PathBuf;
use std::process::{Command, Output};

const MARLIN: &str = include_str!("../examples/driver_marlin.cfg");
const KLIPPER: &str = include_str!("../examples/driver_klipper.cfg");

/// The example without the lines of the homing mode, so that a test can give its own.
fn base(example: &str) -> String {
    example
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !(t.starts_with("homing_mode")
                || t.starts_with("homing_sensitivity")
                || t.starts_with("# homing_sensitivity"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn with(extra: &str) -> String {
    format!("{}\n{extra}\n", base(MARLIN))
}

/// The text with the line of a key replaced by other lines.
fn swap(text: &str, key: &str, new: &str) -> String {
    text.lines()
        .map(|l| {
            if l.trim_start().starts_with(&format!("{key} =")) {
                new.to_string()
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(
            "
",
        )
}

fn error_of(text: &str) -> eh3d_phoenix_pnp::gcodedriver::ProfileError {
    DriverProfile::parse(text).unwrap_err()
}

const SENSORLESS: &str =
    "homing_mode = sensorless\nhoming_sensitivity_x = 80\nhoming_sensitivity_y = 75";

#[test]
fn the_examples_home_with_switches_and_the_mode_is_a_word_of_a_short_list() {
    for text in [MARLIN, KLIPPER] {
        let p = DriverProfile::parse(text).unwrap();
        assert_eq!(p.homing_mode, HomingMode::Switch);
        assert_eq!(p.homing_sensitivity, [None, None]);
        assert_eq!(p.home_lines(), p.home);
    }
    assert_eq!(HomingMode::Sensorless.name(), "sensorless");
    let e = error_of(&with("homing_mode = magnets"));
    assert_eq!(e.key, "homing_mode");
    assert!(matches!(e.fault, Fault::BadValue(_)), "{e:?}");
    let p = DriverProfile::parse(&with(SENSORLESS)).unwrap();
    assert_eq!(p.homing_mode, HomingMode::Sensorless);
    assert_eq!(p.homing_sensitivity, [Some(80), Some(75)]);
    // the case of the word does not matter
    assert!(DriverProfile::parse(&with(&SENSORLESS.replace("sensorless", "Sensorless"))).is_ok());
}

#[test]
fn homing_without_switches_needs_a_sensitivity_for_each_axis_and_switches_take_none() {
    let e = error_of(&with("homing_mode = sensorless"));
    assert_eq!(e.key, "homing_sensitivity_x");
    assert!(matches!(e.fault, Fault::Missing(_)), "{e:?}");
    let e = error_of(&with("homing_mode = sensorless\nhoming_sensitivity_x = 80"));
    assert_eq!(e.key, "homing_sensitivity_y");
    assert!(matches!(e.fault, Fault::Missing(_)), "{e:?}");
    // a threshold has no use with end switches, and keeping it would hide that the mode was not the one meant
    let e = error_of(&with("homing_sensitivity_x = 80"));
    assert_eq!(e.key, "homing_mode");
    assert!(matches!(e.fault, Fault::Order(_)), "{e:?}");
}

#[test]
fn the_sensitivity_is_a_whole_number_in_its_range_and_the_line_has_the_places_for_it() {
    for (line, ok) in [
        ("homing_sensitivity_x = -64", true),
        ("homing_sensitivity_x = 255", true),
        ("homing_sensitivity_x = -65", false),
        ("homing_sensitivity_x = 256", false),
        ("homing_sensitivity_x = 12.5", false),
        ("homing_sensitivity_x = strong", false),
    ] {
        let text = with(&format!(
            "homing_mode = sensorless\nhoming_sensitivity_y = 1\n{line}"
        ));
        assert_eq!(DriverProfile::parse(&text).is_ok(), ok, "{line}");
    }
    for line in [
        "homing_sensitivity_line = M914 X80",
        "homing_sensitivity_line = M914 {axis}",
        "homing_sensitivity_line = M914 {value}",
        "homing_sensitivity_line = M914 {axis}{value} ; note",
    ] {
        let e = error_of(&with(line));
        assert_eq!(e.key, "homing_sensitivity_line", "{line}");
        assert!(matches!(e.fault, Fault::BadValue(_)), "{line}: {e:?}");
    }
    // a key twice is an error as for every key that is not a list
    let e = error_of(&with(&format!("{SENSORLESS}\nhoming_sensitivity_x = 70")));
    assert_eq!(e.fault, Fault::Duplicate);
}

#[test]
fn the_sensitivities_are_the_first_lines_of_the_homing() {
    let p = DriverProfile::parse(&with(SENSORLESS)).unwrap();
    assert_eq!(p.home_lines(), ["M914 X80", "M914 Y75", "G28"]);
    // another letter for an axis, another line for another dialect of the controller
    let text = swap(
        &with(&format!(
            "{SENSORLESS}\nhoming_sensitivity_line = SET_TMC_FIELD STEPPER=stepper_{{axis_lower}} FIELD=sgthrs VALUE={{value}}"
        )),
        "axis_x",
        "axis_x = A",
    );
    let p = DriverProfile::parse(&text).unwrap();
    assert_eq!(
        p.home_lines(),
        [
            "SET_TMC_FIELD STEPPER=stepper_a FIELD=sgthrs VALUE=80",
            "SET_TMC_FIELD STEPPER=stepper_y FIELD=sgthrs VALUE=75",
            "G28"
        ]
    );
    // the lines of home stay as they are, in order, after the sensitivities
    let text = swap(&with(SENSORLESS), "home", "home = G28 X\nhome = G28 Y");
    let p = DriverProfile::parse(&text).unwrap();
    assert_eq!(p.home_lines().len(), 4);
    assert_eq!(&p.home_lines()[2..], ["G28 X", "G28 Y"]);
    // a negative sensitivity is written with its sign
    let text =
        with("homing_mode = sensorless\nhoming_sensitivity_x = -3\nhoming_sensitivity_y = 0");
    assert_eq!(
        DriverProfile::parse(&text).unwrap().home_lines()[..2],
        ["M914 X-3", "M914 Y0"]
    );
}

#[test]
fn the_simulated_controller_is_given_the_sensitivities_before_it_homes() {
    for (name, text) in [
        ("marlin", with(SENSORLESS)),
        (
            "klipper",
            swap(
                &with(&format!(
                    "{SENSORLESS}\nhoming_sensitivity_line = SET_TMC_FIELD STEPPER=stepper_{{axis_lower}} FIELD=sgthrs VALUE={{value}}"
                )),
                "dialect",
                "dialect = klipper",
            ),
        ),
    ] {
        let p = DriverProfile::parse(&text).unwrap();
        let mut s = Session::new(&p, SimLink::new(GcodeSim::new(&p, Faults::default(), 1)));
        s.connect().unwrap();
        assert_eq!(s.link.sim.sensitivity(0), None, "{name}");
        s.home().unwrap();
        assert_eq!(s.link.sim.sensitivity(0), Some(80), "{name}");
        assert_eq!(s.link.sim.sensitivity(1), Some(75), "{name}");
        let log = &s.link.sim.log;
        let at = |needle: &str| log.iter().position(|l| l.contains(needle)).unwrap();
        assert!(at("80") < at("G28") && at("75") < at("G28"), "{name}: {log:?}");
    }
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

fn profile_file(name: &str, text: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("phoenix_pnp_homing_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("profile.cfg");
    std::fs::write(&file, text).unwrap();
    file
}

#[test]
fn homing_repeat_tells_the_sensitivities_and_judges_the_spread_from_a_fixed_seed() {
    let file = profile_file("repeat", &with(SENSORLESS));
    let f = file.to_str().unwrap();
    let o = run(&["homing-repeat", f, "--runs", "8", "--seed", "4"]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    let text = out(&o);
    assert!(
        text.contains("homing without switches: sensitivity 80 in X and 75 in Y"),
        "{text}"
    );
    assert!(text.contains("8 homings: spread 0 mm"), "{text}");
    // a controller that lands apart is over the limit, and the same seed gives the same figures
    let args = [
        "homing-repeat",
        f,
        "--runs",
        "12",
        "--jitter",
        "0.4",
        "--seed",
        "4",
    ];
    let (a, b) = (run(&args), run(&args));
    assert_eq!(a.status.code(), Some(2), "{}", err(&a));
    assert!(err(&a).contains("is over"));
    assert_eq!(out(&a), out(&b));
    let o = run(&["homing-repeat", f, "--lang", "es", "--seed", "4"]);
    assert!(
        out(&o).contains("referenciado sin finales de carrera: sensibilidad 80 en X y 75 en Y"),
        "{}",
        out(&o)
    );
    // a machine with switches says nothing of sensitivities
    let o = run(&["homing-repeat", "examples/driver_marlin.cfg", "--runs", "4"]);
    assert!(!out(&o).contains("without switches"), "{}", out(&o));
    let _ = std::fs::remove_dir_all(file.parent().unwrap());
}

#[test]
fn driver_check_refuses_homing_without_switches_that_has_no_sensitivity() {
    let file = profile_file("check", &with("homing_mode = sensorless"));
    let o = run(&["driver-check", file.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(2));
    assert!(err(&o).contains("sensitivity"), "{}", err(&o));
    let _ = std::fs::remove_dir_all(file.parent().unwrap());
}
