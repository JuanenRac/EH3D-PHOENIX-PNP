//! The commands of the files for assembly, run as a person runs them with the example files: the placement list and the bill, the check of a list, the conversion of a position file, the names of
//! the footprints, the search, the checks for assembly, the stencil, and the two views of a journal.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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
    let dir =
        std::env::temp_dir().join(format!("phoenix_pnp_fabcli_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

const FEATURES: &str = "kind,id,x_mm,y_mm,w_mm,h_mm,side\n\
outline,board,0,0,100,80,\n\
pad,U1,10,10,0.3,0.8,top\n\
pad,U1,10.5,10,0.3,0.8,top\n\
pad,C1,30,30,1.2,1.2,top\n\
hole,via,30,30,0.3,0.3,top\n\
pad,T1,50,50,0.2,0.2,top\n\
pad,H1,60,40,3,3,top\n\
tooling,T1,10,2,2,2,\n";

const JOURNAL: &str = "step,at_s,took_s,action,reference,feeder,nozzle,result,status\n\
1,0.000,1.000,pick,R1,F1,1,ok,pending\n\
2,1.000,2.000,place,R1,F1,1,ok,placed\n\
3,9.000,1.000,pick,R2,F1,1,ok,pending\n\
4,10.000,2.000,place,R2,F1,1,PHOENIX-PNP-024,pending\n";

#[test]
fn export_fab_prints_the_placement_list_and_writes_both_files_when_asked() {
    let o = run(&["export-fab", "examples/board_dnp.csv"]);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    assert!(text.starts_with("Designator,Mid X,Mid Y,Layer,Rotation,Customer Supply\n"));
    assert!(text.contains("R2,15.0000mm,20.0000mm,Top,90.00,DNP"));
    let dir = temp("export");
    let (cpl, bom) = (dir.join("cpl.csv"), dir.join("bom.csv"));
    let o = run(&[
        "export-fab",
        "examples/board_dnp.csv",
        "--bom",
        "examples/bom.csv",
        "--cpl-out",
        p(&cpl),
        "--bom-out",
        p(&bom),
        "--mount",
        "--dnp-mode",
        "omit",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(
        out(&o).contains("5 parts in the placement list"),
        "{}",
        out(&o)
    );
    let list = std::fs::read_to_string(&cpl).unwrap();
    assert!(list.starts_with("Designator,Mid X,Mid Y,Layer,Rotation,Mount\n"));
    assert!(!list.contains("R2,"));
    let bill = std::fs::read_to_string(&bom).unwrap();
    assert!(bill.starts_with("MPN,Designator,Quantity,Description,Footprint\n"));
    assert!(bill.contains("CC0603KRX7R9BB104"), "{bill}");
    // a file that exists is never written over
    let o = run(&["export-fab", "examples/board_dnp.csv", "--cpl-out", p(&cpl)]);
    assert!(!o.status.success());
}

#[test]
fn export_fab_takes_one_side_and_refuses_a_wrong_option() {
    let o = run(&["export-fab", "examples/board_dnp.csv", "--side", "bottom"]);
    assert!(o.status.success());
    assert_eq!(out(&o).lines().count(), 2);
    for bad in [
        vec!["export-fab", "examples/board_dnp.csv", "--side", "left"],
        vec![
            "export-fab",
            "examples/board_dnp.csv",
            "--dnp-mode",
            "maybe",
        ],
        vec!["export-fab", "examples/board_dnp.csv", "--nothing"],
        vec!["export-fab"],
    ] {
        let o = run(&bad);
        assert!(!o.status.success(), "{bad:?}");
        assert!(err(&o).starts_with("error: "), "{}", err(&o));
    }
}

#[test]
fn fab_check_accepts_what_export_fab_writes_and_names_the_lines_it_refuses() {
    let dir = temp("check");
    let good = dir.join("good.csv");
    let o = run(&[
        "export-fab",
        "examples/board_dnp.csv",
        "--cpl-out",
        p(&good),
    ]);
    assert!(o.status.success());
    let o = run(&["fab-check", p(&good)]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("would be accepted"));
    let bad = dir.join("bad.csv");
    std::fs::write(
        &bad,
        "Designator,Mid X,Mid Y,Layer,Rotation\n\"R1,R2\",1mm,2mm,Top,0\nR3,1mm,2mm,Side,0\n",
    )
    .unwrap();
    let o = run(&["fab-check", p(&bad)]);
    assert_eq!(o.status.code(), Some(2));
    let text = out(&o);
    assert!(
        text.contains("line 2: ") && text.contains("line 3: "),
        "{text}"
    );
    assert!(err(&o).contains("2 lines would be refused"));
}

#[test]
fn convert_cpl_reads_the_dialects_and_writes_the_one_asked_for() {
    let o = run(&["convert-cpl", "examples/altium_pnp.csv"]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(err(&o).contains("read as altium: 7 parts"), "{}", err(&o));
    assert!(
        out(&o).contains("R1,10.0000mm,20.0000mm,Top,0.00,"),
        "{}",
        out(&o)
    );
    let o = run(&[
        "convert-cpl",
        "examples/altium_pnp.csv",
        "--flip-y",
        "--origin",
        "10,0",
        "--rot-offset",
        "90",
        "--to",
        "kicad-pos",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    assert!(text.contains("## Unit = mm"));
    let r1 = text.lines().find(|l| l.starts_with("R1 ")).unwrap();
    let f: Vec<&str> = r1.split_whitespace().collect();
    // x 10 - 10, y 20 flipped, rotation 0 negated and turned 90
    assert_eq!((f[3], f[4], f[5]), ("0.0000", "-20.0000", "90.0000"));
    let o = run(&[
        "convert-cpl",
        "examples/board.mnt",
        "--from",
        "eagle",
        "--to",
        "generic",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).starts_with("reference,x(mm),y(mm),rotation,side,value,package\n"));
    for bad in [
        vec!["convert-cpl", "examples/altium_pnp.csv", "--to", "gerber"],
        vec!["convert-cpl", "examples/altium_pnp.csv", "--from", "orcad"],
        vec!["convert-cpl", "examples/altium_pnp.csv", "--origin", "10"],
        vec!["convert-cpl", "examples/altium_pnp.csv", "--bottom", "up"],
        vec!["convert-cpl", "examples/bom.csv"],
    ] {
        assert!(!run(&bad).status.success(), "{bad:?}");
    }
}

#[test]
fn convert_cpl_takes_its_defaults_from_the_settings() {
    let dir = temp("convertcfg");
    let cfg = dir.join("machine.cfg");
    std::fs::write(
        &cfg,
        "cpl_origin_x_mm = 10\ncpl_flip_y = yes\ncpl_rotation_offset_deg = 90\n",
    )
    .unwrap();
    let o = run(&[
        "convert-cpl",
        "examples/altium_pnp.csv",
        "--config",
        p(&cfg),
        "--to",
        "generic",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("R1,0.0000,-20.0000,"), "{}", out(&o));
    std::fs::write(&cfg, "cpl_flip_y = maybe\n").unwrap();
    let o = run(&[
        "convert-cpl",
        "examples/altium_pnp.csv",
        "--config",
        p(&cfg),
    ]);
    assert!(!o.status.success());
    assert!(err(&o).contains("cpl_flip_y"), "{}", err(&o));
}

#[test]
fn ipc_name_reads_makes_and_refuses() {
    let o = run(&["ipc-name", "R_0603", "RESC1608X55N", "--density", "M"]);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    assert!(
        text.contains("R_0603 -> RESC1608X50M (made from the package"),
        "{text}"
    );
    assert!(
        text.contains("RESC1608X55N -> RESC1608X55M (read"),
        "{text}"
    );
    let o = run(&["ipc-name", "SOIC127P600X175-8N"]);
    assert!(out(&o).contains("SOIC127P600X175-8N -> SOIC127P600X175-8N"));
    let o = run(&[
        "ipc-name", "--make", "SOIC", "--pitch", "1.27", "--span", "6", "--height", "1.75",
        "--pins", "8",
    ]);
    assert_eq!(out(&o).trim(), "SOIC127P600X175-8N");
    let o = run(&[
        "ipc-name",
        "--make",
        "resc",
        "--length",
        "1.6",
        "--width",
        "0.8",
        "--height",
        "0.55",
        "--density",
        "L",
    ]);
    assert_eq!(out(&o).trim(), "RESC1608X55L");
    let o = run(&[
        "ipc-name", "--make", "QFP", "--pitch", "0.5", "--length", "10", "--width", "10",
        "--height", "1.6", "--pins", "48",
    ]);
    assert_eq!(out(&o).trim(), "QFP50P1000X1000X160-48N");
    let o = run(&["ipc-name", "R_0603", "banana"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(err(&o).contains("banana"));
    assert!(out(&o).contains("RESC1608X50N"));
    for bad in [
        vec!["ipc-name"],
        vec!["ipc-name", "--make", "SOIC", "--pitch", "1.27"],
        vec![
            "ipc-name", "--make", "RESC", "--length", "20", "--width", "1", "--height", "1",
        ],
        vec!["ipc-name", "R_0603", "--density", "Z"],
    ] {
        assert!(!run(&bad).status.success(), "{bad:?}");
    }
}

#[test]
fn footprint_find_lists_the_names_of_the_table_that_are_the_query() {
    let o = run(&[
        "footprint-find",
        "Resistor_SMD:R_0603_1608Metric",
        "QFN32",
        "--packages",
        "examples/packages.csv",
    ]);
    let text = out(&o);
    assert!(text.contains("  R_0603  (same footprint)"), "{text}");
    assert!(text.contains("QFN32:") && text.contains("QFN-32"), "{text}");
    let o = run(&[
        "footprint-find",
        "ZZZZZZ",
        "--packages",
        "examples/packages.csv",
        "--distance",
        "0",
    ]);
    assert_eq!(o.status.code(), Some(2));
    assert!(out(&o).contains("no name of the table matches"));
    assert!(!run(&["footprint-find"]).status.success());
    assert!(!run(&["footprint-find", "R", "--limit", "0"])
        .status
        .success());
}

#[test]
fn dfm_check_reports_in_the_language_asked_for_and_exits_with_two_on_a_warning() {
    let dir = temp("dfm");
    let features = dir.join("features.csv");
    std::fs::write(&features, FEATURES).unwrap();
    let o = run(&[
        "dfm-check",
        "examples/board_dnp.csv",
        p(&features),
        "--packages",
        "examples/packages.csv",
    ]);
    assert_eq!(o.status.code(), Some(2), "{}", err(&o));
    let text = out(&o);
    assert!(
        text.contains("[error] C1: a hole lies inside a pad"),
        "{text}"
    );
    assert!(text.contains("[note] U1: pads 0.5 mm apart"), "{text}");
    assert!(text.contains("dfm_verified = off"));
    let o = run(&[
        "dfm-check",
        "examples/board_dnp.csv",
        p(&features),
        "--lang",
        "es",
    ]);
    assert!(
        out(&o).contains("hay un agujero dentro de un pad"),
        "{}",
        out(&o)
    );
    let o = run(&[
        "dfm-check",
        "examples/board_dnp.csv",
        p(&features),
        "--lang",
        "ja",
    ]);
    assert!(out(&o).contains("穴があります"));
    let csv = dir.join("dfm.csv");
    let o = run(&[
        "dfm-check",
        "examples/board_dnp.csv",
        p(&features),
        "--lang",
        "pt",
        "--out",
        p(&csv),
    ]);
    assert!(out(&o).contains("written to"));
    assert!(std::fs::read_to_string(&csv)
        .unwrap()
        .contains("hole_in_pad"));
    let o = run(&[
        "dfm-check",
        "examples/board_dnp.csv",
        p(&features),
        "--out",
        p(&csv),
    ]);
    assert!(!o.status.success());
}

#[test]
fn dfm_check_takes_its_limits_from_the_settings() {
    let dir = temp("dfmcfg");
    let features = dir.join("features.csv");
    std::fs::write(&features, "kind,id,x_mm,y_mm,w_mm\ntab,left,0,0,3\n").unwrap();
    let o = run(&["dfm-check", "examples/board_dnp.csv", p(&features)]);
    assert_eq!(o.status.code(), Some(2));
    let cfg = dir.join("machine.cfg");
    std::fs::write(&cfg, "dfm_rail_min_mm = 2\ndfm_verified = yes\n").unwrap();
    let o = run(&[
        "dfm-check",
        "examples/board_dnp.csv",
        p(&features),
        "--config",
        p(&cfg),
    ]);
    assert!(o.status.success(), "{}", out(&o));
    assert!(out(&o).contains("dfm_verified = on"));
}

#[test]
fn stencil_apertures_gives_the_openings_and_exits_with_two_on_a_warning() {
    let dir = temp("stencil");
    let features = dir.join("features.csv");
    std::fs::write(&features, FEATURES).unwrap();
    let csv = dir.join("openings.csv");
    let o = run(&[
        "stencil-apertures",
        p(&features),
        "--out",
        p(&csv),
        "--lang",
        "de",
    ]);
    assert_eq!(o.status.code(), Some(2), "{}", err(&o));
    let text = out(&o);
    assert!(
        text.contains("das Flächenverhältnis einer Öffnung"),
        "{text}"
    );
    let rows = std::fs::read_to_string(&csv).unwrap();
    assert!(rows.contains("H1,window,"));
    assert!(rows.contains("U1,pad,10.0000,10.0000,0.2550,"), "{rows}");
    assert!(!run(&["stencil-apertures"]).status.success());
}

#[test]
fn sim_busy_and_sim_tower_read_the_journal_of_a_simulated_job() {
    let dir = temp("views");
    let journal = dir.join("journal.csv");
    std::fs::write(&journal, JOURNAL).unwrap();
    let o = run(&["sim-busy", p(&journal)]);
    assert_eq!(o.status.code(), Some(2));
    let text = out(&o);
    assert!(
        text.contains("12.000 s from the first step to the last, 6.000 s of waiting"),
        "{text}"
    );
    assert!(text.contains("nozzle 1: 6.000 s in 4 steps"), "{text}");
    assert!(
        text.contains("the machine waited 6.000 s after step 2"),
        "{text}"
    );
    let o = run(&["sim-tower", p(&journal)]);
    assert_eq!(o.status.code(), Some(2));
    let text = out(&o);
    assert!(
        text.contains("3.000 s to 9.000 s: yellow (waiting)"),
        "{text}"
    );
    assert!(text.contains("red (failed)"), "{text}");
    let csv = dir.join("tower.csv");
    let o = run(&["sim-tower", p(&journal), "--out", p(&csv)]);
    assert!(out(&o).contains("written to"));
    assert!(std::fs::read_to_string(&csv)
        .unwrap()
        .starts_with("from_s,to_s,light,why\n"));
    let fine = dir.join("fine.csv");
    std::fs::write(&fine, "step,at_s,took_s,action,reference,feeder,nozzle,result,status\n1,0,1,pick,R1,F1,1,ok,pending\n").unwrap();
    assert!(run(&["sim-busy", p(&fine)]).status.success());
    assert!(run(&["sim-tower", p(&fine)]).status.success());
    assert!(!run(&["sim-busy", p(&dir.join("missing.csv"))])
        .status
        .success());
}

#[test]
fn the_help_names_the_new_commands() {
    let o = run(&["help"]);
    let text = out(&o);
    for command in [
        "export-fab",
        "fab-check",
        "convert-cpl",
        "ipc-name",
        "footprint-find",
        "dfm-check",
        "stencil-apertures",
        "sim-busy",
        "sim-tower",
    ] {
        assert!(
            text.contains(&format!("phoenix-pnp {command} ")),
            "{command}"
        );
    }
}

#[test]
fn the_example_features_and_journal_are_read_by_the_commands() {
    let o = run(&[
        "dfm-check",
        "examples/board_dnp.csv",
        "examples/features.csv",
        "--packages",
        "examples/packages_bodies.csv",
    ]);
    assert_eq!(o.status.code(), Some(2));
    let text = out(&o);
    assert!(
        text.contains("[error] C1: a hole lies inside a pad"),
        "{text}"
    );
    assert!(text.contains("[warning] R1: a track ends"), "{text}");
    assert!(text.contains("[note] U1: pads 0.5 mm apart"), "{text}");
    assert!(
        text.contains("3 findings: 1 errors, 1 warnings, 1 notes (5 parts, 8 pads)"),
        "{text}"
    );
    let o = run(&["stencil-apertures", "examples/features.csv"]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("11 openings (4 of them windows of thermal pads), 0 warnings"));
    let o = run(&["sim-busy", "examples/journal.csv"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        out(&o).contains("13.500 s from the first step to the last"),
        "{}",
        out(&o)
    );
    let o = run(&["sim-tower", "examples/journal.csv"]);
    assert!(
        out(&o).contains("4.900 s to 9.100 s: yellow (slow step)"),
        "{}",
        out(&o)
    );
}
