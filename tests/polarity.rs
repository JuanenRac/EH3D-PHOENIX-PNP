//! The sheet of the parts that have a direction: which parts are on it, their order, the CSV, the settings and the command `polarity-sheet`.

use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::parts::{from_csv, Part, Side};
use eh3d_phoenix_pnp::polarity::{is_polarized, sheet, summary, to_csv};
use std::path::PathBuf;
use std::process::{Command, Output};

const MACHINE: &str = include_str!("../examples/machine.cfg");

const BOARD: &str = "ref,val,package,x,y,rot,side
R1,10k,R_0603,10,10,0,top
C1,100n,C_0603,12,10,0,top
D10,1N4148,SOD-123,14,10,90,top
D2,1N4148,SOD-123,16,10,180,top
LED1,red,LED_0603,18,10,0,top
U1,MCU,QFN-32,20,10,0,top
C7,10u,CP_Elec_6.3x5.4,22,10,0,top
Y1,8MHz,Crystal_3225,24,10,0,top
J1,USB,USB_C,26,10,0,bottom
Q1,BC847,SOT-23,28,10,270,bottom
R2,1k,R_0603,30,10,0,bottom
";

fn parts() -> Vec<Part> {
    from_csv(BOARD).unwrap()
}

fn lists() -> (Vec<String>, Vec<String>) {
    let c = MachineConfig::default();
    (c.polarity_refs, c.polarity_packages)
}

fn names(rows: &[Part]) -> Vec<&str> {
    rows.iter().map(|p| p.reference.as_str()).collect()
}

#[test]
fn the_parts_with_a_direction_are_found_by_reference_or_by_package() {
    let (refs, packages) = lists();
    let rows = sheet(&parts(), &refs, &packages);
    // top first, then bottom; C7 before the Ds, D2 before D10; resistors, a plain capacitor and a crystal are not on it
    assert_eq!(names(&rows), ["C7", "D2", "D10", "LED1", "U1", "J1", "Q1"]);
}

#[test]
fn either_list_is_enough_and_an_empty_list_turns_its_rule_off() {
    let p = parts();
    let by_package = ["*sot*".to_string()];
    assert_eq!(names(&sheet(&p, &[], &by_package)), ["Q1"]);
    let by_ref = ["y*".to_string()];
    assert_eq!(names(&sheet(&p, &by_ref, &[])), ["Y1"]);
    assert!(sheet(&p, &[], &[]).is_empty());
    // the patterns are matched without regard to case
    let upper = ["SOD-*".to_string()];
    assert_eq!(names(&sheet(&p, &[], &upper)), ["D2", "D10"]);
    assert!(is_polarized(&p[0], &["r?".to_string()], &[]));
    assert!(!is_polarized(&p[0], &["r??".to_string()], &[]));
}

#[test]
fn the_sheet_is_a_csv_with_an_empty_column_to_tick() {
    let (refs, packages) = lists();
    let csv = to_csv(&sheet(&parts(), &refs, &packages));
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(
        lines[0],
        "reference,value,package,side,x_mm,y_mm,rotation_deg,checked"
    );
    assert_eq!(lines[1], "C7,10u,CP_Elec_6.3x5.4,top,22.000,10.000,0.0,");
    assert_eq!(lines[2], "D2,1N4148,SOD-123,top,16.000,10.000,180.0,");
    assert_eq!(lines[3], "D10,1N4148,SOD-123,top,14.000,10.000,90.0,");
    assert_eq!(lines.len(), 8);
    assert_eq!(lines[7], "Q1,BC847,SOT-23,bottom,28.000,10.000,270.0,");
    assert!(lines[1..].iter().all(|l| l.ends_with(',')));
}

#[test]
fn a_value_with_a_comma_is_quoted() {
    let p =
        from_csv("ref,val,package,x,y,rot,side\nD1,\"1N4148, fast\",SOD-123,1,2,0,top\n").unwrap();
    let (refs, packages) = lists();
    let csv = to_csv(&sheet(&p, &refs, &packages));
    assert!(csv.contains("D1,\"1N4148, fast\",SOD-123,top,"), "{csv}");
}

#[test]
fn the_summary_counts_both_sides() {
    let (refs, packages) = lists();
    let rows = sheet(&parts(), &refs, &packages);
    assert_eq!(
        summary(&rows, 11),
        "7 of 11 parts have a direction (5 on the top side, 2 on the bottom side)"
    );
    assert_eq!(rows.iter().filter(|p| p.side == Side::Bottom).count(), 2);
}

#[test]
fn the_numbers_in_a_reference_are_compared_as_numbers() {
    let text = "ref,val,package,x,y,rot,side\nD100,a,SOD,0,0,0,top\nD9,a,SOD,0,0,0,top\nD11,a,SOD,0,0,0,top\nD1A,a,SOD,0,0,0,top\n";
    let (refs, packages) = lists();
    let rows = sheet(&from_csv(text).unwrap(), &refs, &packages);
    assert_eq!(names(&rows), ["D1A", "D9", "D11", "D100"]);
}

#[test]
fn the_settings_have_their_defaults_their_limits_and_the_example_is_the_default() {
    for key in ["polarity_refs", "polarity_packages"] {
        assert!(KEYS.contains(&key), "{key}");
    }
    let e = MachineConfig::parse(MACHINE).unwrap();
    let d = MachineConfig::default();
    assert_eq!(e.polarity_refs, d.polarity_refs);
    assert_eq!(e.polarity_packages, d.polarity_packages);
    assert_eq!(d.polarity_refs.len(), 7);
    let c = MachineConfig::parse("polarity_refs = X*, Y?\npolarity_packages =").unwrap();
    assert_eq!(c.polarity_refs, ["x*", "y?"]);
    assert!(c.polarity_packages.is_empty());
    let many = (0..51)
        .map(|i| format!("p{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let e = MachineConfig::parse(&format!("polarity_refs = {many}")).unwrap_err();
    assert_eq!(e.code, Code::ConfigBadValue);
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

fn text(o: &[u8]) -> String {
    String::from_utf8_lossy(o).into_owned()
}

#[test]
fn polarity_sheet_prints_the_example_board_and_keeps_one_side() {
    let o = run(&["polarity-sheet", "examples/board.pos"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    // R1 to R3 and C1 and C2 have no direction; U1 and D1 do
    assert_eq!(out.lines().count(), 3, "{out}");
    assert!(
        out.contains("U1,MCU,QFN-32,top,40.000,25.000,0.0,"),
        "{out}"
    );
    assert!(
        out.contains("D1,LED,LED_0603,bottom,30.000,10.000,"),
        "{out}"
    );
    assert!(text(&o.stderr)
        .contains("ok: 2 of 7 parts have a direction (1 on the top side, 1 on the bottom side)"));
    let top = run(&["polarity-sheet", "examples/board.pos", "--side", "top"]);
    assert_eq!(text(&top.stdout).lines().count(), 2);
    assert!(
        text(&top.stderr).contains("1 of 6 parts"),
        "{}",
        text(&top.stderr)
    );
    assert!(
        !run(&["polarity-sheet", "examples/board.pos", "--side", "left"])
            .status
            .success()
    );
}

#[test]
fn polarity_sheet_writes_a_new_file_only_and_follows_the_configuration() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_polarity_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (out, cfg) = (dir.join("sheet.csv"), dir.join("m.cfg"));
    std::fs::write(&cfg, "polarity_refs = r1\npolarity_packages =\n").unwrap();
    let args = [
        "polarity-sheet",
        "examples/board.pos",
        "--config",
        cfg.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ];
    let o = run(&args);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(text(&o.stdout).starts_with("ok: 1 of 7 parts have a direction"));
    let written = std::fs::read_to_string(&out).unwrap();
    assert_eq!(written.lines().count(), 2);
    assert!(written.contains("R1,10k,R_0603,top,"));
    let again = run(&args);
    assert!(!again.status.success());
    assert!(text(&again.stderr).contains("PHOENIX-PNP-040"));
    assert_eq!(std::fs::read_to_string(&out).unwrap(), written);
    assert!(!run(&["polarity-sheet"]).status.success());
    assert!(!run(&["polarity-sheet", "examples/board.pos", "--oops"])
        .status
        .success());
    let _ = std::fs::remove_dir_all(&dir);
}
