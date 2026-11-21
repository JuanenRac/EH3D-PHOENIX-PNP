//! The wiring sheet: the pin of an actuator read from its lines, the table and the drawing of the pneumatics from a driver profile, the problems that make the command end with 2, and the
//! files it writes.

use eh3d_phoenix_pnp::gcodedriver::DriverProfile;
use eh3d_phoenix_pnp::wiring::{build, pin_of};
use std::process::Command;

const MARLIN: &str = include_str!("../examples/driver_marlin.cfg");
const KLIPPER: &str = include_str!("../examples/driver_klipper.cfg");

#[test]
fn the_pin_is_read_from_the_lines_of_marlin_klipper_and_others() {
    assert_eq!(pin_of("M42 P5 S255").as_deref(), Some("P5"));
    assert_eq!(pin_of("SET_PIN PIN=pump VALUE=1").as_deref(), Some("pump"));
    assert_eq!(pin_of("M106 P2 S255").as_deref(), Some("P2"));
    // a line with no pin is known by its code
    assert_eq!(pin_of("M7").as_deref(), Some("M7"));
    assert_eq!(pin_of("  M8  ").as_deref(), Some("M8"));
    assert_eq!(pin_of("hello"), None);
    assert_eq!(pin_of(""), None);
    assert_eq!(pin_of("PIN="), None);
}

#[test]
fn a_marlin_profile_gives_the_axes_the_pump_and_the_valve_with_what_a_stop_lets_go() {
    let sheet = build(&DriverProfile::parse(MARLIN).unwrap());
    assert_eq!(sheet.dialect, "marlin");
    assert!(sheet.problems.is_empty(), "{:?}", sheet.problems);
    let roles: Vec<&str> = sheet.rows.iter().map(|r| r.role).collect();
    assert_eq!(roles, ["axis", "axis", "axis", "axis", "pump", "release"]);
    let pump = sheet.rows.iter().find(|r| r.role == "pump").unwrap();
    assert_eq!((pump.component.as_str(), pump.pin.as_str()), ("pump", "P5"));
    assert_eq!(pump.at_stop, "yes");
    let blow = sheet.rows.iter().find(|r| r.role == "release").unwrap();
    assert_eq!(blow.pin, "P6");
    assert_eq!(blow.on, "M42 P6 S255");
    let rot = sheet
        .rows
        .iter()
        .find(|r| r.component == "axis rotation")
        .unwrap();
    assert_eq!(rot.pin, "driver E");
    // the CSV has a header and one row for each thing
    let csv = sheet.csv();
    assert!(csv.starts_with("component,role,pin,on,off,at_stop\n"));
    assert_eq!(csv.lines().count(), 1 + sheet.rows.len());
    assert!(sheet.text().contains("evidence=simulator"));
}

#[test]
fn a_klipper_profile_names_its_pins_and_a_profile_with_a_shared_pin_is_a_problem() {
    let sheet = build(&DriverProfile::parse(KLIPPER).unwrap());
    assert_eq!(sheet.dialect, "klipper");
    assert!(sheet.problems.is_empty(), "{:?}", sheet.problems);
    let pins: Vec<&str> = sheet
        .rows
        .iter()
        .filter(|r| r.role != "axis")
        .map(|r| r.pin.as_str())
        .collect();
    assert_eq!(pins, ["pump", "blow"]);
    // two actuators on one pin
    let shared = MARLIN
        .replace("\r\n", "\n")
        .replace("M42 P6 S255", "M42 P5 S255")
        .replace("M42 P6 S0", "M42 P5 S0");
    let sheet = build(&DriverProfile::parse(&shared).unwrap());
    assert!(
        sheet.problems.iter().any(|p| p.contains("used by another")),
        "{:?}",
        sheet.problems
    );
}

#[test]
fn the_drawing_is_well_formed_and_has_a_nozzle_for_each_one_of_the_profile() {
    let mut profile = DriverProfile::parse(MARLIN).unwrap();
    profile.nozzles = 3;
    let svg = build(&profile).svg();
    assert!(svg.starts_with("<svg ") && svg.trim_end().ends_with("</svg>"));
    for n in 1..=3 {
        assert!(svg.contains(&format!("nozzle {n}")), "{svg}");
    }
    assert!(!svg.contains("nozzle 4"));
    assert!(svg.contains("pin P5") && svg.contains("pin P6"));
    // a name with a mark of XML is written out, and the drawing stays one document
    let mut odd = DriverProfile::parse(MARLIN).unwrap();
    odd.actuators[1].name = "blow<&>".into();
    odd.release = "blow<&>".into();
    let svg = build(&odd).svg();
    assert!(
        svg.contains("blow&lt;&amp;&gt;") && !svg.contains("blow<"),
        "{svg}"
    );
}

fn run(args: &[&str]) -> (i32, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    (
        o.status.code().unwrap(),
        String::from_utf8_lossy(&o.stdout).into_owned(),
    )
}

#[test]
fn the_command_prints_writes_never_over_a_file_and_ends_with_2_for_a_problem() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_wiring_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let csv = dir.join("w.csv");
    let svg = dir.join("w.svg");
    let (code, out) = run(&[
        "wiring-sheet",
        "examples/driver_marlin.cfg",
        "--out",
        csv.to_str().unwrap(),
        "--svg",
        svg.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("wiring sheet: marlin controller"));
    assert!(std::fs::read_to_string(&csv)
        .unwrap()
        .contains("pump,pump,P5"));
    assert!(std::fs::read_to_string(&svg).unwrap().contains("</svg>"));
    // a file that exists is never written over
    let (code, _) = run(&[
        "wiring-sheet",
        "examples/driver_marlin.cfg",
        "--out",
        csv.to_str().unwrap(),
    ]);
    assert_eq!(code, 1);
    // no pump: the sheet is not whole
    let no_pump = MARLIN
        .replace("\r\n", "\n")
        .replace("release = blow\n", "release = none\n");
    let p = dir.join("no_pump.cfg");
    std::fs::write(&p, no_pump).unwrap();
    let (code, out) = run(&["wiring-sheet", p.to_str().unwrap()]);
    assert_eq!(code, 2, "{out}");
    assert!(
        out.contains("problem: the profile names no blow-off valve"),
        "{out}"
    );
    // a file that is not there could not be run
    assert_eq!(run(&["wiring-sheet", "no_such.cfg"]).0, 1);
    let _ = std::fs::remove_dir_all(&dir);
}
