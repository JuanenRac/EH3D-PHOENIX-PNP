//! The specification sheet of the machine: what the owner wrote in the settings is shown, what nobody measured says so in eight languages and no figure is made up, and a report of a job carries
//! the sheet when at least one field is given.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::job::Job;
use eh3d_phoenix_pnp::machsheet::{from_config, not_measured, title, Field};
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use eh3d_phoenix_pnp::report::{self, build, ReportInput};
use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Output};

const GIVEN: &str = "spec_table = Granite 600 x 400\nspec_machine = Desktop, two heads\nspec_work_area = 400 x 300 mm\nspec_smallest_part = 0402\nspec_max_speed_mm_s = 250\n";

fn cfg(text: &str) -> MachineConfig {
    MachineConfig::parse(text).unwrap()
}

#[test]
fn nothing_given_is_nothing_measured_in_every_language() {
    let sheet = from_config(&cfg(""));
    assert_eq!(sheet.given(), 0);
    assert!(sheet.values.iter().all(|(_, v)| v.is_none()));
    let mut words = std::collections::BTreeSet::new();
    for lang in Lang::ALL {
        let text = sheet.text(lang);
        assert_eq!(
            text.matches(&format!(": {}", not_measured(lang))).count(),
            5,
            "{lang:?}: {text}"
        );
        assert!(!title(lang).is_empty());
        words.insert(not_measured(lang));
    }
    // eight languages, eight different words
    assert_eq!(words.len(), 8, "{words:?}");
    assert_eq!(not_measured(Lang::En), "not measured");
    assert_eq!(not_measured(Lang::Es), "sin medir");
    assert_eq!(not_measured(Lang::Pt), "por medir");
    assert!(sheet.text(Lang::En).contains("0 of 5 fields are given"));
}

#[test]
fn what_the_settings_give_is_shown_as_written() {
    let sheet = from_config(&cfg(GIVEN));
    assert_eq!(sheet.given(), 5);
    let rows = sheet.rows(Lang::En);
    assert_eq!(
        rows,
        [
            ("Table".to_string(), "Granite 600 x 400".to_string()),
            ("Machine".to_string(), "Desktop, two heads".to_string()),
            ("Work area".to_string(), "400 x 300 mm".to_string()),
            ("Smallest part tried".to_string(), "0402".to_string()),
            ("Greatest speed".to_string(), "250 mm/s".to_string()),
        ]
    );
    assert!(sheet
        .text(Lang::Pt)
        .contains("Área de trabalho: 400 x 300 mm"));
    assert!(sheet.text(Lang::En).contains("5 of 5 fields are given"));
    let json = sheet.json();
    assert_eq!(json["max_speed_mm_s"].as_f64(), Some(250.0));
    assert_eq!(json["smallest_part"], "0402");
    // one field given: the others stay null in the JSON and say so in the text
    let one = from_config(&cfg("spec_machine = Bench\n"));
    assert_eq!(one.given(), 1);
    assert_eq!(one.json()["table"], Value::Null);
    assert_eq!(one.json()["max_speed_mm_s"], Value::Null);
    assert!(one.text(Lang::De).contains("Tisch: nicht gemessen"));
    assert_eq!(Field::ALL.len(), 5);
}

#[test]
fn the_limits_and_the_speeds_of_the_plan_are_settings_and_never_a_measurement() {
    let sheet = from_config(&cfg(
        "limit_x_max_mm = 900\nlimit_y_max_mm = 700\nspeed_mm_s = 300\nx_accel_mm_s2 = 3000\n",
    ));
    assert_eq!(sheet.given(), 0, "{:?}", sheet.values);
}

#[test]
fn a_bad_line_of_the_sheet_is_refused_when_the_settings_are_read() {
    for bad in [
        "spec_max_speed_mm_s = -1",
        "spec_max_speed_mm_s = fast",
        "spec_max_speed_mm_s = 100001",
        "spec_table = a\u{7}b",
    ] {
        assert!(MachineConfig::parse(bad).is_err(), "{bad}");
    }
    let long = format!("spec_work_area = {}", "x".repeat(81));
    assert!(MachineConfig::parse(&long).is_err());
    assert!(MachineConfig::parse(&format!("spec_work_area = {}", "x".repeat(80))).is_ok());
}

// ---- in the report of a job

const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nC1,100n,C,10,20,0,top\n";
const FEEDERS: &str = "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\n";
const PACKAGES: &str = "package,nozzle_tip\nR,N45\nC,N45\n";

fn job(extra: &str) -> Job {
    let cfg = MachineConfig::parse(&format!(
        "tip_changer_x_mm = -50\ntip_changer_y_mm = 0\n{extra}"
    ))
    .unwrap();
    let parts = from_csv(PARTS).unwrap();
    let feeders = feeding::from_csv(FEEDERS).unwrap();
    let picks = assign(&parts, &feeders, Some(Side::Top));
    let mut catalog = Catalog::new();
    catalog.add_packages(PACKAGES).unwrap();
    let plan = plan_advanced(
        &parts,
        &feeders,
        &picks,
        &catalog,
        &cfg,
        &Options::default(),
    )
    .unwrap();
    Job::new(plan, feeders, cfg).unwrap()
}

fn input_of(j: &Job) -> ReportInput {
    ReportInput {
        machine: Some(from_config(j.config())),
        ..ReportInput::default()
    }
}

#[test]
fn the_report_of_a_job_carries_the_sheet_only_when_a_field_is_given() {
    let mut with = job(GIVEN);
    with.run(None).unwrap();
    let r = build(&with, &input_of(&with));
    let text = r.text(Lang::En);
    assert!(
        text.contains("Machine\n-------\nTable: Granite 600 x 400"),
        "{text}"
    );
    assert!(text.contains("Greatest speed: 250 mm/s"), "{text}");
    let json: Value = serde_json::from_str(&report::json(&with, &input_of(&with))).unwrap();
    assert_eq!(json["work"]["machine"]["work_area"], "400 x 300 mm");
    let csv = report::csv(&with, &input_of(&with));
    assert!(csv.contains("machine,smallest_part,0402,"), "{csv}");
    assert!(r.html(Lang::En).contains("Granite 600 x 400"));
    // in another language the labels follow
    assert!(r.text(Lang::Es).contains("Velocidad máxima: 250 mm/s"));
    // nothing given: no section, and the report is the one it always was
    let mut none = job("");
    none.run(None).unwrap();
    let plain = build(&none, &ReportInput::default()).text(Lang::En);
    let with_empty = build(&none, &input_of(&none)).text(Lang::En);
    assert_eq!(plain, with_empty);
    assert!(!with_empty.contains("not measured"));
    // a part of the sheet given, the rest says so
    let mut some = job("spec_machine = Bench\n");
    some.run(None).unwrap();
    let text = build(&some, &input_of(&some)).text(Lang::En);
    assert!(text.contains("Machine: Bench") && text.contains("Table: not measured"));
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

#[test]
fn the_command_prints_the_sheet_of_the_settings_it_is_given() {
    let o = run(&["machine-sheet", "--config", "examples/machine.cfg"]);
    let text = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(o.status.success(), "{text}");
    assert!(text.contains("Table: not measured") && text.contains("0 of 5 fields are given"));
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_sheet_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("machine.cfg");
    std::fs::write(&file, GIVEN).unwrap();
    let f = file.to_str().unwrap();
    let o = run(&["machine-sheet", "--config", f, "--lang", "fr"]);
    let text = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(
        text.contains("Zone de travail: 400 x 300 mm") && text.contains("5 champs sur 5"),
        "{text}"
    );
    assert_eq!(run(&["machine-sheet", "extra"]).status.code(), Some(1));
    assert_eq!(
        run(&["machine-sheet", "--lang", "xx"]).status.code(),
        Some(1)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
