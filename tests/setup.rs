//! The setup sheet: the feeders to load by slot, the parts that are missing, the nozzle tips and their wear, the calibrations the job depends on, the warnings, the eight languages, the
//! command `setup-sheet` and the tool of the Tools tab.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::{Code, Lang};
use eh3d_phoenix_pnp::feeding::{self, FeederKind};
use eh3d_phoenix_pnp::gui::toolbox::{setup_text, Tool, Toolbox};
use eh3d_phoenix_pnp::gui::workspace::Workspace;
use eh3d_phoenix_pnp::nozzlelife::NozzleLife;
use eh3d_phoenix_pnp::parts::{parse, Part, Side};
use eh3d_phoenix_pnp::setup::{build, Calibration, Inputs, PanelInfo, Warning, Wear};
use std::path::PathBuf;
use std::process::{Command, Output};

const BOARD: &str = include_str!("../examples/board.pos");
const PACKAGES: &str = include_str!("../examples/packages.csv");
const FEEDERS: &str = "id,value,package,x,y,count,slot\nF1,10k,R_0603,-20,10,100,S2\nF2,100n,C_0603,-20,20,100,S1\nF3,MCU,QFN-32,-20,40,10,S3\nF4,LED,LED_0603,-20,50,50,\n";

fn parts() -> Vec<Part> {
    parse(BOARD).unwrap()
}

fn catalog() -> Catalog {
    let mut c = Catalog::new();
    c.add_packages(PACKAGES).unwrap();
    c
}

fn sheet_of(
    feeders_text: &str,
    cfg_text: &str,
    side: Option<Side>,
    boards: usize,
    panel: Option<PanelInfo>,
    life: Option<&NozzleLife>,
) -> eh3d_phoenix_pnp::setup::SetupSheet {
    let (p, f, c, cfg) = (
        parts(),
        feeding::from_csv(feeders_text).unwrap(),
        catalog(),
        MachineConfig::parse(cfg_text).unwrap(),
    );
    build(&Inputs {
        parts: &p,
        feeders: &f,
        catalog: &c,
        cfg: &cfg,
        side,
        boards,
        panel,
        life,
    })
    .unwrap()
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

fn text(o: &[u8]) -> String {
    String::from_utf8_lossy(o).into_owned()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_setup_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_feeders_to_load_are_the_ones_the_board_needs_in_the_order_of_their_slots() {
    let s = sheet_of(FEEDERS, "", Some(Side::Top), 1, None, None);
    let ids: Vec<(&str, &str)> = s
        .feeders
        .iter()
        .map(|f| (f.slot.as_str(), f.id.as_str()))
        .collect();
    // the LED is on the bottom side: its feeder is not needed for the top
    assert_eq!(ids, [("S1", "F2"), ("S2", "F1"), ("S3", "F3")]);
    assert_eq!((s.boards, s.parts_per_board, s.kinds), (1, 6, 3));
    assert!(s.feeders.iter().all(|f| f.kind == FeederKind::Tape));
    let qfn = s.feeders.iter().find(|f| f.id == "F3").unwrap();
    assert_eq!(
        (qfn.tape_mm, qfn.tip.as_str(), qfn.count),
        (12.0, "N75", 10)
    );
    // both sides: the feeder that names no slot comes last
    let both = sheet_of(FEEDERS, "", None, 1, None, None);
    let last = both.feeders.last().unwrap();
    assert_eq!((last.id.as_str(), last.slot.as_str()), ("F4", ""));
    assert_eq!(both.parts_per_board, 7);
    // a feeder that is not on the board's list is not on the sheet
    let extra = format!("{FEEDERS}F9,1M,R_0805,0,0,10,S9\n");
    let more = sheet_of(&extra, "", None, 1, None, None);
    assert!(more.feeders.iter().all(|f| f.id != "F9"));
}

#[test]
fn what_the_feeders_do_not_hold_for_the_run_is_missing_and_the_sheet_is_not_ready() {
    let ready = sheet_of(FEEDERS, "", Some(Side::Top), 1, None, None);
    assert!(ready.is_ready() && ready.missing.is_empty() && ready.short_parts() == 0);
    let big = sheet_of(FEEDERS, "", Some(Side::Top), 40, None, None);
    assert!(!big.is_ready());
    let lines: Vec<(String, u64, u64, u64)> = big
        .missing
        .iter()
        .map(|m| (m.value.clone(), m.needed, m.held, m.short))
        .collect();
    // the capacitors are enough; the order is the one of the packages
    assert_eq!(
        lines,
        [
            ("MCU".to_string(), 40, 10, 30),
            ("10k".to_string(), 120, 100, 20)
        ]
    );
    assert_eq!(big.short_parts(), 50);
    let t = big.text(Lang::En);
    assert!(
        t.contains("NOT ready: parts are missing for the run."),
        "{t}"
    );
    assert!(t.contains("10k / R_0603"), "{t}");
}

#[test]
fn the_sheet_is_in_the_eight_languages_and_the_csv_does_not_change_with_the_language() {
    let s = sheet_of(FEEDERS, "", Some(Side::Top), 40, None, None);
    let by_lang = [
        (Lang::En, "SETUP SHEET", "Feeders to load", "NOT ready"),
        (
            Lang::Es,
            "HOJA DE PREPARACIÓN",
            "Alimentadores que cargar",
            "NO está listo",
        ),
        (
            Lang::De,
            "RÜSTLISTE",
            "Zuführungen zu bestücken",
            "NICHT bereit",
        ),
        (
            Lang::Fr,
            "FICHE DE PRÉPARATION",
            "Alimentateurs à charger",
            "PAS prêt",
        ),
        (
            Lang::It,
            "FOGLIO DI PREPARAZIONE",
            "Alimentatori da caricare",
            "NON pronto",
        ),
        (Lang::Ja, "段取りシート", "セットするフィーダー", "未準備"),
        (Lang::Zh, "准备清单", "需装载的供料器", "未就绪"),
        (
            Lang::Pt,
            "FOLHA DE PREPARAÇÃO",
            "Alimentadores a carregar",
            "NÃO está pronto",
        ),
    ];
    let mut seen = std::collections::BTreeSet::new();
    for (lang, title, section, not_ready) in by_lang {
        let t = s.text(lang);
        assert!(t.starts_with(title), "{lang:?}: {t}");
        assert!(t.contains(section), "{lang:?}: {t}");
        assert!(t.contains(not_ready), "{lang:?}: {t}");
        assert!(t.contains("F1") && t.contains("S2"), "{lang:?}: {t}");
        assert!(
            seen.insert(t),
            "{lang:?} gives the text of another language"
        );
    }
    let csv = s.csv();
    assert_eq!(
        csv.lines().next().unwrap(),
        "slot,feeder,kind,value,package,tape_mm,tip,parts,x,y"
    );
    assert!(
        csv.contains("\nS3,F3,tape,MCU,QFN-32,12,N75,10,-20,40\n"),
        "{csv}"
    );
    assert_eq!(csv.lines().count(), 4);
    // the summary says whether the run can be loaded and how many calibrations are not set
    let summary = s.summary(Lang::En);
    assert!(summary.starts_with("NOT ready"), "{summary}");
    assert!(summary.contains("NOT SET / "), "{summary}");
}

#[test]
fn the_tips_of_the_run_say_how_worn_they_are_against_the_level_of_the_warning() {
    let life =
        NozzleLife::parse("tip,uses,mounts,replaced\nN45,499996,9,0\nN75,510000,3,0\n").unwrap();
    let s = sheet_of(FEEDERS, "", Some(Side::Top), 1, None, Some(&life));
    let tips: Vec<(&str, u64, Option<u64>, Wear)> = s
        .tips
        .iter()
        .map(|t| (t.tip.as_str(), t.parts, t.used, t.wear))
        .collect();
    // N45 places the 3 resistors and the 2 capacitors of the board: 499996 and 5 more is at the level; N75 is over it already
    assert_eq!(
        tips,
        [
            ("N45", 5, Some(499_996), Wear::Reaches),
            ("N75", 1, Some(510_000), Wear::Over)
        ]
    );
    assert_eq!(s.tips[0].kinds, ["100n / C_0603", "10k / R_0603"]);
    let t = s.text(Lang::En);
    assert!(
        t.contains("<- look at this tip (the run takes it to the level of the warning)"),
        "{t}"
    );
    assert!(t.contains("<- look at this tip\n"), "{t}");
    // without a record nothing is said about the wear, and a level of 0 says nothing either
    let none = sheet_of(FEEDERS, "", Some(Side::Top), 1, None, None);
    assert!(none
        .tips
        .iter()
        .all(|t| t.used.is_none() && t.wear == Wear::Fine));
    let off = sheet_of(
        FEEDERS,
        "nozzle_life_warn = 0",
        Some(Side::Top),
        1,
        None,
        Some(&life),
    );
    assert!(off
        .tips
        .iter()
        .all(|t| t.wear == Wear::Fine && t.used.is_some()));
    // a tip that is not in the record has been used 0 times
    let empty = NozzleLife::default();
    let fresh = sheet_of(FEEDERS, "", Some(Side::Top), 1, None, Some(&empty));
    assert!(fresh.tips.iter().all(|t| t.used == Some(0)));
}

#[test]
fn the_calibrations_the_job_depends_on_are_listed_with_whether_they_are_set() {
    let items = |s: &eh3d_phoenix_pnp::setup::SetupSheet| -> Vec<(Calibration, bool)> {
        s.calibrations.iter().map(|c| (c.item, c.set)).collect()
    };
    // the default machine: two tips and no changer, no camera, no runout, no slack
    let plain = sheet_of(FEEDERS, "", Some(Side::Top), 1, None, None);
    assert_eq!(
        items(&plain),
        [
            (Calibration::NozzleRunout, false),
            (Calibration::Backlash, false),
            (Calibration::TipChanger, false)
        ]
    );
    assert_eq!(plain.calibrations_not_set(), 3);
    // everything set, the camera used and a panel with its marks measured
    let cfg = "align = on\ncamera_units_per_px = 0.02\nnozzle_runout_mm = 0.1\nbacklash_x_mm = 0.05\ntip_changer_x_mm = -50\ntip_changer_y_mm = 0";
    let full = sheet_of(
        FEEDERS,
        cfg,
        Some(Side::Top),
        1,
        Some(PanelInfo {
            fiducials_measured: true,
        }),
        None,
    );
    assert_eq!(
        items(&full),
        [
            (Calibration::Camera, true),
            (Calibration::NozzleRunout, true),
            (Calibration::Backlash, true),
            (Calibration::TipChanger, true),
            (Calibration::Fiducials, true)
        ]
    );
    assert_eq!(full.calibrations_not_set(), 0);
    // a panel without measured marks, and the camera of the pipeline without a calibration
    let open = sheet_of(
        FEEDERS,
        "align = on",
        Some(Side::Top),
        1,
        Some(PanelInfo {
            fiducials_measured: false,
        }),
        None,
    );
    assert!(items(&open).contains(&(Calibration::Camera, false)));
    assert!(items(&open).contains(&(Calibration::Fiducials, false)));
    let t = open.text(Lang::En);
    assert!(
        t.contains("Camera calibration: NOT SET (camera-calibrate)"),
        "{t}"
    );
    assert!(
        t.contains("Fiducials of the panel measured: NOT SET (--fiducials)"),
        "{t}"
    );
    for c in [
        Calibration::Camera,
        Calibration::NozzleRunout,
        Calibration::Backlash,
        Calibration::TipChanger,
        Calibration::Fiducials,
    ] {
        assert!(!c.key().is_empty() && !c.how().is_empty());
    }
}

#[test]
fn a_feeder_that_is_off_and_a_bulk_feeder_with_a_part_that_has_a_direction_are_told() {
    let feeders = "id,value,package,x,y,count,kind,enabled\nF1,10k,R_0603,-20,10,100,tape,yes\nF2,100n,C_0603,-20,20,100,tape,no\nB1,MCU,QFN-32,-20,40,10,bulk,yes\n";
    let s = sheet_of(feeders, "", Some(Side::Top), 1, None, None);
    assert!(
        s.warnings.contains(&Warning::FeederOff("F2".to_string())),
        "{:?}",
        s.warnings
    );
    assert!(
        s.warnings.contains(&Warning::BulkDirection(
            "B1".to_string(),
            vec!["U1".to_string()]
        )),
        "{:?}",
        s.warnings
    );
    // the feeder that is off holds the capacitors, which are then missing
    assert!(!s.is_ready());
    assert!(s.missing.iter().any(|m| m.value == "100n" && m.held == 0));
    let t = s.text(Lang::En);
    assert!(t.contains("the feeder F2 is switched off"), "{t}");
    assert!(
        t.contains("the bulk feeder B1 holds parts that have a direction (U1)"),
        "{t}"
    );
    let de = s.text(Lang::De);
    assert!(de.contains("Schüttgut-Zuführung B1"), "{de}");
    // a bulk feeder of parts with no direction is fine
    let fine = "id,value,package,x,y,count,kind\nF1,10k,R_0603,-20,10,100,bulk\nF2,100n,C_0603,-20,20,100,tube\nF3,MCU,QFN-32,-20,40,10,tape\n";
    let ok = sheet_of(fine, "", Some(Side::Top), 1, None, None);
    assert!(ok.warnings.is_empty(), "{:?}", ok.warnings);
    assert!(ok.text(Lang::En).contains("Warnings\n  none\n"));
}

#[test]
fn a_number_of_boards_out_of_range_is_refused() {
    let (p, f, c, cfg) = (
        parts(),
        feeding::from_csv(FEEDERS).unwrap(),
        catalog(),
        MachineConfig::default(),
    );
    for boards in [0, 1_000_001] {
        let e = build(&Inputs {
            parts: &p,
            feeders: &f,
            catalog: &c,
            cfg: &cfg,
            side: None,
            boards,
            panel: None,
            life: None,
        })
        .unwrap_err();
        assert_eq!(e.code, Code::PlanSetting, "{boards}");
    }
}

#[test]
fn the_command_prints_the_sheet_in_the_language_asked_and_writes_the_csv_once() {
    let base = [
        "setup-sheet",
        "examples/board.pos",
        "examples/feeders.csv",
        "--packages",
        "examples/packages.csv",
        "--config",
        "examples/machine.cfg",
    ];
    let o = run(&base);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    assert!(out.starts_with("SETUP SHEET"), "{out}");
    assert!(
        out.contains("F1") && out.contains("QFN-32") && out.contains("N75"),
        "{out}"
    );
    assert!(out.contains("Ready to load"), "{out}");
    // a run of 40 boards is more than the reels hold: the exit code is 1
    let mut big: Vec<&str> = base.to_vec();
    big.extend(["--boards", "40"]);
    let o = run(&big);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        text(&o.stderr).contains("the feeders lack"),
        "{}",
        text(&o.stderr)
    );
    assert!(text(&o.stdout).contains("NOT ready"), "{}", text(&o.stdout));
    // the language
    let mut es: Vec<&str> = base.to_vec();
    es.extend(["--lang", "es"]);
    let o = run(&es);
    assert!(
        text(&o.stdout).starts_with("HOJA DE PREPARACIÓN"),
        "{}",
        text(&o.stdout)
    );
    // the wear of the tips from a record
    let mut worn: Vec<&str> = base.to_vec();
    worn.extend(["--life", "examples/nozzle_life.csv"]);
    let o = run(&worn);
    assert!(text(&o.stdout).contains("510000"), "{}", text(&o.stdout));
    assert!(
        text(&o.stdout).contains("<- look at this tip"),
        "{}",
        text(&o.stdout)
    );
    // the CSV is written once, and never over one of the files it was made from
    let dir = scratch("csv");
    let path = dir.join("sheet.csv").to_string_lossy().into_owned();
    let mut with_out: Vec<&str> = base.to_vec();
    with_out.extend(["--out", path.as_str()]);
    let o = run(&with_out);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let csv = std::fs::read_to_string(&path).unwrap();
    assert!(
        csv.starts_with("slot,feeder,kind,value,package,tape_mm,tip,parts,x,y\n"),
        "{csv}"
    );
    let again = run(&with_out);
    assert_eq!(again.status.code(), Some(1));
    assert!(
        text(&again.stderr).contains("PHOENIX-PNP-040"),
        "{}",
        text(&again.stderr)
    );
    let mut same: Vec<&str> = base.to_vec();
    same.extend(["--out", "examples/feeders.csv"]);
    assert_eq!(run(&same).status.code(), Some(1));
    // a record of the tips that is not valid, and a number of boards that is not one
    let mut junk: Vec<&str> = base.to_vec();
    junk.extend(["--life", "examples/board.pos"]);
    let o = run(&junk);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        text(&o.stderr).contains("PHOENIX-PNP-059"),
        "{}",
        text(&o.stderr)
    );
    let mut zero: Vec<&str> = base.to_vec();
    zero.extend(["--boards", "0"]);
    let o = run(&zero);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        text(&o.stderr).contains("PHOENIX-PNP-017"),
        "{}",
        text(&o.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_command_reads_a_panel_and_asks_for_its_fiducials() {
    let o = run(&[
        "setup-sheet",
        "--panel",
        "examples/panel.json",
        "examples/feeders.csv",
        "--packages",
        "examples/packages.csv",
    ]);
    let out = text(&o.stdout);
    assert!(
        out.contains("Fiducials of the panel measured: NOT SET (--fiducials)"),
        "{out}"
    );
    let with_marks = run(&[
        "setup-sheet",
        "--panel",
        "examples/panel.json",
        "--fiducials",
        "examples/fid_measured.csv",
        "examples/feeders.csv",
        "--packages",
        "examples/packages.csv",
    ]);
    // the marks of that example are not the marks of this panel: the command says so instead of printing a sheet
    assert!(
        text(&with_marks.stdout).contains("Fiducials of the panel measured: set")
            || with_marks.status.code() == Some(1),
        "{}{}",
        text(&with_marks.stdout),
        text(&with_marks.stderr)
    );
}

#[test]
fn the_tool_of_the_tools_tab_gives_the_summary_and_writes_the_sheet() {
    let mut ws = Workspace::new(Lang::En);
    ws.load_text("board.pos", BOARD).unwrap();
    ws.load_text("feeders.csv", include_str!("../examples/feeders.csv"))
        .unwrap();
    ws.load_text("packages.csv", PACKAGES).unwrap();
    let (info, sheet) = setup_text(&ws, Some(Side::Top), 1, Lang::En).unwrap();
    assert!(info.starts_with("Ready to load"), "{info}");
    assert!(
        info.contains("Calibrations: ") && info.contains("Warnings: 0"),
        "{info}"
    );
    assert!(
        sheet.starts_with("SETUP SHEET") && sheet.contains("N75"),
        "{sheet}"
    );
    let (info, _) = setup_text(&ws, None, 40, Lang::Es).unwrap();
    assert!(info.starts_with("NO está listo"), "{info}");
    assert!(setup_text(&ws, None, 0, Lang::En)
        .unwrap_err()
        .contains("PHOENIX-PNP-017"));
    // an excluded part is not on the sheet
    ws.excluded.insert("U1".to_string());
    let (_, less) = setup_text(&ws, Some(Side::Top), 1, Lang::En).unwrap();
    assert!(!less.contains("QFN-32"), "{less}");
    // the tool runs and is written under its name, to be printed
    let dir = scratch("tool");
    let mut tb = Toolbox::new(&MachineConfig::default());
    tb.tool = Tool::Setup;
    tb.boards = 40;
    tb.run(&ws, Lang::En);
    assert!(tb.error.is_empty(), "{}", tb.error);
    assert!(tb.info.starts_with("NOT ready"), "{}", tb.info);
    let path = tb.save(dir.to_str().unwrap(), Lang::En).unwrap();
    assert!(path.ends_with("setup_sheet.txt"), "{path}");
    assert!(std::fs::read_to_string(&path)
        .unwrap()
        .starts_with("SETUP SHEET"));
    assert_eq!(Tool::from_key("setup"), Some(Tool::Setup));
    assert!(!Tool::Setup.reads_text());
    let _ = std::fs::remove_dir_all(&dir);
}
