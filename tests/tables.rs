//! The parts that are not to be mounted (a column of the table of positions, or `--dnp`), the tables of the examples with their checks (`tables-check`) and the report of a simulated job as a CSV.

use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::{Code, Lang};
use eh3d_phoenix_pnp::gui::workspace::Workspace;
use eh3d_phoenix_pnp::job::STATS_CSV_HEADER;
use eh3d_phoenix_pnp::parts::{from_csv, parse, parse_all, take_references, Profile};
use eh3d_phoenix_pnp::preflight::{Severity, Source};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BOARD_DNP: &str = include_str!("../examples/board_dnp.csv");
const BOARD_POS: &str = include_str!("../examples/board.pos");

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn example(name: &str) -> String {
    root()
        .join("examples")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eh3d_pnp_tables_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .output()
        .unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn path(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

fn references(parts: &[eh3d_phoenix_pnp::parts::Part]) -> Vec<&str> {
    parts.iter().map(|p| p.reference.as_str()).collect()
}

/// One part with the cell of the column of mounting given, for the tests of the words.
fn one_part(column: &str, cell: &str) -> String {
    format!("ref,val,package,x,y,rot,side,{column}\nR1,10k,R_0603,1,2,0,top,{cell}\n")
}

#[test]
fn a_dnp_column_leaves_the_marked_parts_out_and_keeps_them_apart() {
    let parsed = parse_all(BOARD_DNP, &Profile::default()).unwrap();
    // R2 says yes and C2 says x; R3 says no and the empty cells mount the part
    assert_eq!(references(&parsed.parts), ["R1", "R3", "C1", "U1", "D1"]);
    assert_eq!(references(&parsed.dnp), ["R2", "C2"]);
    // what the file says about a part left out is kept whole
    assert_eq!(parsed.dnp[0].x, 15.0);
    assert_eq!(parsed.dnp[0].rotation, 90.0);
    assert_eq!(parsed.dnp[1].rotation, 270.0);
    // the readers that give only the parts to mount agree
    assert_eq!(from_csv(BOARD_DNP).unwrap(), parsed.parts);
}

#[test]
fn a_fitted_column_says_the_opposite() {
    let text = "ref,val,package,x,y,rot,side,Fitted\nR1,10k,R_0603,1,2,0,top,no\nR2,10k,R_0603,3,4,0,top,YES\nR3,10k,R_0603,5,6,0,top,\n";
    let parsed = parse_all(text, &Profile::default()).unwrap();
    assert_eq!(references(&parsed.parts), ["R2", "R3"]);
    assert_eq!(references(&parsed.dnp), ["R1"]);
    // the column that says "do not mount" wins when a file has both
    let both = "ref,val,package,x,y,rot,side,fitted,dnp\nR1,10k,R_0603,1,2,0,top,yes,yes\n";
    let parsed = parse_all(both, &Profile::default()).unwrap();
    assert_eq!(references(&parsed.dnp), ["R1"]);
}

#[test]
fn the_words_for_yes_and_no_are_the_usual_ones_and_those_of_the_configuration() {
    for yes in ["yes", "Y", "true", "1", "x", "X"] {
        let parsed = parse_all(&one_part("dnp", yes), &Profile::default()).unwrap();
        assert_eq!((parsed.parts.len(), parsed.dnp.len()), (0, 1), "{yes}");
    }
    for no in ["no", "N", "False", "0", "-", ""] {
        let parsed = parse_all(&one_part("dnp", no), &Profile::default()).unwrap();
        assert_eq!((parsed.parts.len(), parsed.dnp.len()), (1, 0), "{no}");
    }
    let own = Profile {
        extra_yes: vec!["si".to_string()],
        extra_no: vec!["nao".to_string()],
        extra_dnp: vec!["exclude".to_string()],
        ..Profile::default()
    };
    let parsed = parse_all(&one_part("Exclude", "SI"), &own).unwrap();
    assert_eq!(parsed.dnp.len(), 1);
    let parsed = parse_all(&one_part("Exclude", "nao"), &own).unwrap();
    assert_eq!(parsed.parts.len(), 1);
    // without the configuration those words and that name are not known
    assert!(parse_all(&one_part("dnp", "si"), &Profile::default()).is_err());
    let parsed = parse_all(&one_part("exclude", "yes"), &Profile::default()).unwrap();
    assert_eq!((parsed.parts.len(), parsed.dnp.len()), (1, 0));
}

#[test]
fn a_word_that_is_neither_yes_nor_no_is_an_error_with_its_code_and_row() {
    let text = "ref,val,package,x,y,rot,side,dnp\nR1,10k,R_0603,1,2,0,top,no\nR2,10k,R_0603,3,4,0,top,maybe\n";
    let e = parse_all(text, &Profile::default()).unwrap_err();
    assert!(e.starts_with("PHOENIX-PNP-033: "), "{e}");
    assert!(e.contains("row 3") && e.contains("'maybe'"), "{e}");
    // a word in both lists is not understood either
    let both = Profile {
        extra_yes: vec!["maybe".to_string()],
        extra_no: vec!["maybe".to_string()],
        ..Profile::default()
    };
    assert!(parse_all(text, &both).is_err());
}

#[test]
fn the_errors_of_the_file_of_positions_carry_the_code_034() {
    let e = parse_all(
        "Designator,Value,Package,X,Y,Rotation\nR1,1k,0603,1,2,0\n",
        &Profile::default(),
    )
    .unwrap_err();
    assert!(e.starts_with("PHOENIX-PNP-034: "), "{e}");
    assert!(e.contains("side"), "{e}");
    let e = parse_all(
        "## Unit = mm\nR1 10k R_0603 10.0 20.0 top\n",
        &Profile::default(),
    )
    .unwrap_err();
    assert!(e.starts_with("PHOENIX-PNP-034: "), "{e}");
    assert!(e.contains("line 2") && e.contains("6 fields"), "{e}");
    // an error that has its code already keeps it
    let e = parse_all(&one_part("dnp", "perhaps"), &Profile::default()).unwrap_err();
    assert!(e.starts_with("PHOENIX-PNP-033: "), "{e}");
    assert!(!e.contains("PHOENIX-PNP-034"), "{e}");
}

#[test]
fn references_are_taken_out_in_the_order_of_the_file_and_an_unknown_one_is_refused() {
    let mut parts = parse(BOARD_POS).unwrap();
    assert_eq!(
        references(&parts),
        ["R1", "R2", "R3", "C1", "C2", "U1", "D1"]
    );
    let taken = take_references(&mut parts, &["C1".to_string(), "R2".to_string()]).unwrap();
    // in the order of the file, not of the list
    assert_eq!(references(&taken), ["R2", "C1"]);
    assert_eq!(references(&parts), ["R1", "R3", "C2", "U1", "D1"]);
    let before = parts.clone();
    let e = take_references(&mut parts, &["R1".to_string(), "R9".to_string()]).unwrap_err();
    assert!(
        e.starts_with("PHOENIX-PNP-033: ") && e.contains("'R9'"),
        "{e}"
    );
    // nothing was taken: the mistake is found before the first part is touched
    assert_eq!(parts, before);
}

#[test]
fn the_configuration_names_the_columns_and_the_words_of_mounting() {
    let c = MachineConfig::parse(
        "header_dnp = Exclude, Skip\nheader_fitted = Assemble\ndnp_yes_words = SI\ndnp_no_words = Nao, Nein\n",
    )
    .unwrap();
    assert_eq!(c.profile.extra_dnp, ["exclude", "skip"]);
    assert_eq!(c.profile.extra_fitted, ["assemble"]);
    assert_eq!(c.profile.extra_yes, ["si"]);
    assert_eq!(c.profile.extra_no, ["nao", "nein"]);
    assert_eq!(MachineConfig::default().profile, Profile::default());
    let too_many = format!(
        "header_dnp = {}",
        (0..51)
            .map(|i| format!("n{i}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    assert_eq!(
        MachineConfig::parse(&too_many).unwrap_err().code,
        Code::ConfigBadValue
    );
    assert_eq!(
        MachineConfig::parse("dnp_yes_words = a\ndnp_yes_words = b")
            .unwrap_err()
            .code,
        Code::ConfigDuplicateKey
    );
}

#[test]
fn the_codes_33_and_34_have_a_meaning_in_every_language() {
    for code in [Code::MountingBad, Code::PartsBad] {
        let mut seen = std::collections::HashSet::new();
        for lang in Lang::ALL {
            let m = code.message(lang);
            assert!(!m.is_empty());
            seen.insert(m);
        }
        assert_eq!(seen.len(), 8, "{}", code.label());
    }
    assert_eq!(Code::MountingBad.label(), "PHOENIX-PNP-033");
    assert_eq!(Code::PartsBad.label(), "PHOENIX-PNP-034");
    assert_eq!(
        Code::PartsBad.message(Lang::Pt),
        "O ficheiro de posições não é válido"
    );
}

#[test]
fn the_window_keeps_the_parts_left_out_apart_and_says_so_in_the_problems() {
    let parts = "ref,val,package,x,y,rot,side,dnp\nR1,10k,R,10,10,0,top,\nR2,10k,R,20,10,90,top,yes\nR3,10k,R,30,10,180,top,\nC1,100n,C,10,20,0,top,no\nC2,100n,C,20,20,0,top,\nU1,MCU,Q,40,15,0,top,\n";
    let feeders =
        "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\n";
    let mut ws = Workspace::new(Lang::En);
    ws.load_text("parts.csv", parts).unwrap();
    ws.load_text("feeders.csv", feeders).unwrap();
    assert_eq!(references(&ws.parts), ["R1", "R3", "C1", "C2", "U1"]);
    assert_eq!(references(&ws.dnp), ["R2"]);
    let planned = ws.make_plan().unwrap();
    assert_eq!(planned.parts.len(), 5);
    assert_eq!(planned.plan.plan.placements.len(), 5);
    assert!(planned.checks.iter().any(|c| {
        c.severity == Severity::Info
            && c.source == Source::Part("R2".to_string())
            && c.message.contains("not to be mounted")
    }));
    // a file without the column has nothing left out
    ws.load_text(
        "parts.csv",
        "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\n",
    )
    .unwrap();
    assert!(ws.dnp.is_empty());
}

#[test]
fn schedule_leaves_the_parts_marked_dnp_out_and_lists_them() {
    let dir = scratch("dnp_schedule");
    let (plan, report) = (path(&dir, "plan.json"), path(&dir, "report.txt"));
    let o = run(&[
        "schedule",
        &example("board_dnp.csv"),
        &example("feeders.csv"),
        &plan,
        "--config",
        &example("machine.cfg"),
        "--packages",
        &example("packages.csv"),
        "--catalog",
        &example("catalog.csv"),
        "--report-text",
        &report,
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&plan).unwrap()).unwrap();
    let refs: Vec<&str> = v["placements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["reference"].as_str().unwrap())
        .collect();
    // the top side has R1 R3 C1 U1 once R2 and C2 are left out; the microcontroller is of the last block of ranks
    assert_eq!(refs.len(), 4, "{refs:?}");
    assert_eq!(refs[3], "U1");
    assert!(!refs.contains(&"R2") && !refs.contains(&"C2"));
    let text = out(&o);
    assert!(
        text.contains("info: part R2: marked as not to be mounted"),
        "{text}"
    );
    assert!(
        text.contains("info: part C2: marked as not to be mounted"),
        "{text}"
    );
    assert!(text.contains("4 parts in"), "{text}");
    let report = std::fs::read_to_string(&report).unwrap();
    assert!(report.contains("4 parts in"), "{report}");
    assert!(
        report.ends_with("not to be mounted, left out of the plan: 2 parts (R2, C2)\n"),
        "{report}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_dnp_option_leaves_references_out_and_refuses_an_unknown_one() {
    let dir = scratch("dnp_option");
    let plan = path(&dir, "plan.json");
    let base = [
        "schedule",
        &example("board.pos"),
        &example("feeders.csv"),
        &plan,
        "--config",
        &example("machine.cfg"),
        "--packages",
        &example("packages.csv"),
    ];
    let mut with: Vec<&str> = base.to_vec();
    with.extend(["--dnp", "R1, U1"]);
    let o = run(&with);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    assert!(text.contains("4 parts in"), "{text}");
    assert!(text.contains("info: part R1: marked"), "{text}");
    assert!(text.contains("info: part U1: marked"), "{text}");
    let mut wrong: Vec<&str> = base.to_vec();
    wrong.extend(["--dnp", "R1,R9"]);
    let o = run(&wrong);
    assert_eq!(o.status.code(), Some(1));
    let e = err(&o);
    assert!(e.contains("PHOENIX-PNP-033") && e.contains("'R9'"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_panel_lists_the_parts_its_boards_leave_out_once() {
    let dir = scratch("dnp_panel");
    std::fs::copy(example("board_dnp.csv"), path(&dir, "board_dnp.csv")).unwrap();
    let panel = path(&dir, "panel.json");
    std::fs::write(
        &panel,
        r#"{
  "schema_version": 1,
  "boards": { "main": "board_dnp.csv" },
  "panels": {
    "panel": {
      "children": [
        { "id": "A", "board": "main", "x": 0, "y": 0, "rotation": 0 },
        { "id": "B", "board": "main", "x": 60, "y": 0, "rotation": 0 }
      ]
    }
  },
  "root": "panel"
}
"#,
    )
    .unwrap();
    let (plan, report) = (path(&dir, "plan.json"), path(&dir, "report.txt"));
    let o = run(&[
        "schedule",
        "--panel",
        &panel,
        &example("feeders.csv"),
        &plan,
        "--config",
        &example("machine.cfg"),
        "--report-text",
        &report,
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&plan).unwrap()).unwrap();
    // four parts to mount on the top side of each of the two boards
    assert_eq!(v["placements"].as_array().unwrap().len(), 8);
    let report = std::fs::read_to_string(&report).unwrap();
    assert!(
        report.ends_with("not to be mounted, left out of the plan: 2 parts (R2, C2)\n"),
        "{report}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tables_check_reads_the_tables_of_the_examples() {
    let o = run(&[
        "tables-check",
        "--parts",
        &example("board_dnp.csv"),
        "--packages",
        &example("packages_common.csv"),
        "--catalog",
        &example("catalog.csv"),
        "--feeders",
        &example("feeders_kinds.csv"),
        "--slots",
        &example("slots.csv"),
        "--config",
        &example("machine.cfg"),
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    // 7 parts, 12 packages, 5 feeders of the three kinds and 5 slots
    assert!(
        text.contains("ok: parts") && text.contains("(7 rows)"),
        "{text}"
    );
    assert!(
        text.contains("ok: packages") && text.contains("(12 rows)"),
        "{text}"
    );
    assert!(text.contains("ok: catalog"), "{text}");
    assert!(
        text.contains("ok: feeders") && text.contains("(5 rows)"),
        "{text}"
    );
    assert!(text.contains("ok: slots"), "{text}");
    assert_eq!(text.lines().count(), 5, "{text}");
    // the other tables of the examples are valid as well
    let others = run(&[
        "tables-check",
        "--parts",
        &example("board.pos"),
        "--packages",
        &example("packages.csv"),
        "--feeders",
        &example("feeders.csv"),
    ]);
    assert!(others.status.success(), "{}", err(&others));
}

#[test]
fn tables_check_names_the_code_and_the_row_of_every_table_that_is_not_valid() {
    let dir = scratch("tables_bad");
    let feeders = path(&dir, "feeders.csv");
    std::fs::write(
        &feeders,
        "id,value,package,x,y,count\nF1,10k,R_0603,1,2,3\nF2,10k,R_0603,1,x,3\n",
    )
    .unwrap();
    let packages = path(&dir, "packages.csv");
    std::fs::write(&packages, "package,height_mm\nA,1\nB,abc\n").unwrap();
    let parts = path(&dir, "parts.csv");
    std::fs::write(&parts, one_part("dnp", "maybe")).unwrap();
    let o = run(&[
        "tables-check",
        "--feeders",
        &feeders,
        "--packages",
        &packages,
        "--parts",
        &parts,
    ]);
    assert_eq!(o.status.code(), Some(1));
    let text = out(&o);
    // every table is read, in the order parts, packages, feeders
    assert_eq!(text.lines().count(), 3, "{text}");
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[0].starts_with("not valid: parts "), "{text}");
    assert!(
        lines[0].contains("PHOENIX-PNP-033") && lines[0].contains("row 2"),
        "{text}"
    );
    assert!(lines[1].starts_with("not valid: packages "), "{text}");
    assert!(
        lines[1].contains("PHOENIX-PNP-010") && lines[1].contains("row 3"),
        "{text}"
    );
    assert!(lines[2].starts_with("not valid: feeders "), "{text}");
    assert!(
        lines[2].contains("PHOENIX-PNP-013") && lines[2].contains("row 3"),
        "{text}"
    );
    assert!(
        err(&o).contains("3 of 3 tables are not valid"),
        "{}",
        err(&o)
    );
    // the meaning of the code comes in the language asked for
    let es = run(&["tables-check", "--feeders", &feeders, "--lang", "es"]);
    assert!(
        out(&es).contains("Una fila de los alimentadores no es válida"),
        "{}",
        out(&es)
    );
    // nothing to read, a wrong option and an unknown language are refused
    assert_eq!(run(&["tables-check"]).status.code(), Some(1));
    assert_eq!(
        run(&["tables-check", "--tables", "x"]).status.code(),
        Some(1)
    );
    assert_eq!(
        run(&["tables-check", "--feeders", &feeders, "--lang", "xx"])
            .status
            .code(),
        Some(1)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_report_of_a_job_as_a_csv_has_a_header_that_never_changes() {
    assert_eq!(
        STATS_CSV_HEADER,
        "section,name,value,picked,pick_failures,placed,place_failures,faults,disabled"
    );
    let dir = scratch("job_csv");
    let csv = path(&dir, "job.csv");
    let o = run(&[
        "simulate",
        &example("board.pos"),
        &example("feeders.csv"),
        "--config",
        &example("machine.cfg"),
        "--packages",
        &example("packages.csv"),
        "--report-csv",
        &csv,
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let text = std::fs::read_to_string(&csv).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], STATS_CSV_HEADER);
    // every row has the nine columns of the header
    for l in &lines {
        assert_eq!(l.split(',').count(), 9, "{l}");
    }
    // six parts in three kinds, from three feeders, with no failure
    for expected in [
        "total,placed,6,,,,,,",
        "total,deferred,0,,,,,,",
        "total,of,6,,,,,,",
        "total,pick_attempts,6,,,,,,",
        "total,pick_failures,0,,,,,,",
        "package,R_0603,,3,0,3,0,,",
        "package,C_0603,,2,0,2,0,,",
        "package,QFN-32,,1,0,1,0,,",
        "feeder,F1,,3,0,,,0,no",
        "feeder,F2,,2,0,,,0,no",
        "feeder,F3,,1,0,,,0,no",
    ] {
        assert!(lines.contains(&expected), "{expected} is not in\n{text}");
    }
    // the seconds are a number with three decimals
    let seconds = lines
        .iter()
        .find_map(|l| l.strip_prefix("total,seconds,"))
        .unwrap();
    let value = seconds.split(',').next().unwrap();
    assert!(value.parse::<f64>().unwrap() > 0.0, "{value}");
    assert_eq!(value.split('.').nth(1).unwrap().len(), 3, "{value}");
    // a report is never written over a file it was made from
    let same = run(&[
        "simulate",
        &example("board.pos"),
        &example("feeders.csv"),
        "--report-csv",
        &example("feeders.csv"),
    ]);
    assert_eq!(same.status.code(), Some(1));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_parts_put_aside_come_in_the_csv_with_their_reason() {
    let dir = scratch("job_csv_defer");
    let (cfg, csv) = (path(&dir, "failing.cfg"), path(&dir, "job.csv"));
    std::fs::write(
        &cfg,
        "error_policy = defer\nsim_pick_fail_rate = 1\nsim_seed = 3\nfault_limit = 0\n",
    )
    .unwrap();
    let o = run(&[
        "simulate",
        &example("board.pos"),
        &example("feeders.csv"),
        "--config",
        &cfg,
        "--report-csv",
        &csv,
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let text = std::fs::read_to_string(&csv).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines.contains(&"total,placed,0,,,,,,"), "{text}");
    assert!(lines.contains(&"total,deferred,6,,,,,,"), "{text}");
    assert_eq!(
        lines.iter().filter(|l| l.starts_with("deferred,")).count(),
        6,
        "{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
