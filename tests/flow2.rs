//! The height of the nozzle tips, the marks of the boards of a panel, the tapes checked step by step, the effectiveness of a shift, the position files of assemblers and the bills of materials
//! of several boards added up: the library of each one, its command, and boards made by chance on a fixed seed (the numbers are the same on every run; a failure names its seed).

use eh3d_phoenix_pnp::asmexport::{write, Format, Sides, Unit};
use eh3d_phoenix_pnp::asmimport;
use eh3d_phoenix_pnp::bom::Bom;
use eh3d_phoenix_pnp::bommerge::{self, Input};
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::feeders::Feeder;
use eh3d_phoenix_pnp::feeding::{FeederKind, FeederSpec};
use eh3d_phoenix_pnp::job::Phase;
use eh3d_phoenix_pnp::nozzlez::{self, Plan, Reading, Status as ZStatus};
use eh3d_phoenix_pnp::panel::Panel;
use eh3d_phoenix_pnp::panelfids::{self, Status as FStatus};
use eh3d_phoenix_pnp::parts::{parse_all, Part, Side};
use eh3d_phoenix_pnp::runs::{Run, Runs};
use eh3d_phoenix_pnp::shiftoee::{self, Inputs};
use eh3d_phoenix_pnp::sim::Rng;
use eh3d_phoenix_pnp::tapesteps::{self, Limits, Status as TStatus};
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
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_flow2_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn example(name: &str) -> String {
    std::fs::read_to_string(root().join("examples").join(name)).unwrap()
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn below(rng: &mut Rng, n: usize) -> usize {
    (rng.next_u64() % n.max(1) as u64) as usize
}

fn reading(nozzle: &str, z_mm: f64) -> Reading {
    Reading {
        nozzle: nozzle.to_string(),
        z_mm,
    }
}

// ---- the height of the nozzle tips

#[test]
fn the_probings_are_read_with_the_columns_found_by_name() {
    let r = nozzlez::parse_readings("nozzle,z_mm\nN1,0.01\nN1,0.02\nN2,-0.03\n").unwrap();
    assert_eq!(
        r,
        [
            reading("N1", 0.01),
            reading("N1", 0.02),
            reading("N2", -0.03)
        ]
    );
    let r = nozzlez::parse_readings("\u{feff}Tip;Touch_Z_mm\n\nA;0,5\n").map_err(|e| e.to_string());
    // a comma as the decimal sign is not a number: the line is named
    assert!(r.unwrap_err().contains("line 3"));
    let r = nozzlez::parse_readings("id;height_mm\nA;0.5\nB;0.6\n").unwrap();
    assert_eq!((r[0].nozzle.as_str(), r[1].z_mm), ("A", 0.6));
    for (text, why) in [
        ("", "the file is empty"),
        ("nozzle\nN1\n", "no column z_mm"),
        ("z_mm\n1\n", "no column nozzle"),
        ("nozzle,z_mm\n", "no readings"),
        ("nozzle,z_mm\n,1\n", "up to 64 characters"),
        ("nozzle,z_mm\nN1,x\n", "z_mm is a number"),
        ("nozzle,z_mm\nN1,1001\n", "z_mm is a number"),
        ("nozzle,z_mm\nN1,NaN\n", "z_mm is a number"),
        ("nozzle,z_mm\nN1\u{7},1\n", "up to 64 characters"),
    ] {
        let e = nozzlez::parse_readings(text).unwrap_err().to_string();
        assert!(e.contains(why), "{text:?}: {e}");
        assert!(e.contains("PHOENIX-PNP-058"), "{e}");
    }
}

#[test]
fn a_tip_is_called_out_of_line_noisy_or_single_by_the_tolerance() {
    let readings = [
        reading("N1", 0.0),
        reading("N1", 0.01),
        reading("N2", 0.1),
        reading("N2", 0.11),
    ];
    let r = nozzlez::analyse(&readings, 0.02).unwrap();
    assert_eq!(r.rows.len(), 2);
    assert!((r.reference_mm - 0.055).abs() < 1e-12);
    assert!(
        (r.rows[0].offset_mm + 0.05).abs() < 1e-12 && (r.rows[1].offset_mm - 0.05).abs() < 1e-12
    );
    assert!((r.rows[0].spread_mm - 0.01).abs() < 1e-12);
    assert!(r.rows.iter().all(|x| x.status == ZStatus::OutOfLine));
    assert_eq!(r.problems(), 2);
    // a looser tolerance takes both in
    let r = nozzlez::analyse(&readings, 0.06).unwrap();
    assert!(r.rows.iter().all(|x| x.status == ZStatus::Ok));
    assert_eq!(r.problems(), 0);
    // probings that disagree by more than the tolerance are noisy, whatever the offset
    let noisy = [
        reading("A", 0.0),
        reading("A", 0.05),
        reading("B", 0.0),
        reading("B", 0.0),
    ];
    let r = nozzlez::analyse(&noisy, 0.02).unwrap();
    assert_eq!(r.rows[0].status, ZStatus::Noisy);
    assert_eq!(r.rows[1].status, ZStatus::Ok);
    // a tip probed once is told, and one tip alone is its own reference
    let single = [reading("A", 0.2)];
    let r = nozzlez::analyse(&single, 0.02).unwrap();
    assert_eq!(
        (r.rows[0].status, r.rows[0].offset_mm),
        (ZStatus::Single, 0.0)
    );
    for tol in [0.0, 0.0009, 1.01, f64::NAN, -1.0] {
        assert!(nozzlez::analyse(&readings, tol).is_err(), "{tol}");
    }
    assert!(nozzlez::analyse(&[], 0.02).is_err());
}

#[test]
fn the_report_says_the_offset_and_its_correction_with_opposite_signs() {
    let readings = [
        reading("N1", 0.0),
        reading("N1", 0.0),
        reading("N2", 0.04),
        reading("N2", 0.04),
    ];
    let r = nozzlez::analyse(&readings, 0.01).unwrap();
    let text = r.text();
    assert!(
        text.starts_with("2 nozzles, the mean height is 0.0200 mm, tolerance 0.01 mm"),
        "{text}"
    );
    assert!(text.contains("N1: 2 probings, height 0.0000 mm, they differ by 0.0000 mm, offset -0.0200 mm (correction +0.0200 mm) - out_of_line"), "{text}");
    assert!(text.ends_with("2 nozzles need a look\n"), "{text}");
    assert_eq!(
        r.csv(),
        "nozzle,probes,mean_mm,spread_mm,offset_mm,correction_mm,status\nN1,2,0.0000,0.0000,-0.0200,0.0200,out_of_line\nN2,2,0.0400,0.0000,0.0200,-0.0200,out_of_line\n"
    );
    let ok = nozzlez::analyse(&readings, 0.5).unwrap();
    assert!(ok
        .text()
        .ends_with("every nozzle is within the tolerance\n"));
}

#[test]
fn made_up_probings_are_the_same_for_the_same_plan_and_the_offsets_find_the_made_up_heights() {
    let plan = Plan::default();
    let a = nozzlez::simulate(&plan).unwrap();
    assert_eq!(a, nozzlez::simulate(&plan).unwrap());
    assert_eq!(a.readings.len(), 10);
    assert_ne!(
        a,
        nozzlez::simulate(&Plan {
            seed: 2,
            ..plan.clone()
        })
        .unwrap()
    );
    assert!(a.truth.iter().all(|t| t.1.abs() <= plan.spread_mm / 2.0));
    // without noise the probings are the heights, and the offsets are exact
    let exact = nozzlez::simulate(&Plan {
        noise_mm: 0.0,
        nozzles: 5,
        ..Plan::default()
    })
    .unwrap();
    let report = nozzlez::analyse(&exact.readings, 0.5).unwrap();
    assert!(nozzlez::error_against(&report, &exact.truth) < 1e-12);
    assert!(report.rows.iter().all(|r| r.spread_mm == 0.0));
    // with noise and enough probings the offsets are near, on many seeds
    for seed in 0..60u64 {
        let plan = Plan {
            nozzles: 4,
            probes: 25,
            noise_mm: 0.004,
            seed,
            ..Plan::default()
        };
        let made = nozzlez::simulate(&plan).unwrap();
        let report = nozzlez::analyse(&made.readings, 0.5).unwrap();
        let e = nozzlez::error_against(&report, &made.truth);
        assert!(e < 0.004, "seed {seed}: {e}");
    }
}

#[test]
fn the_plan_of_the_probings_is_checked() {
    for plan in [
        Plan {
            nozzles: 0,
            ..Plan::default()
        },
        Plan {
            nozzles: 65,
            ..Plan::default()
        },
        Plan {
            probes: 0,
            ..Plan::default()
        },
        Plan {
            probes: 1001,
            ..Plan::default()
        },
        Plan {
            noise_mm: -0.1,
            ..Plan::default()
        },
        Plan {
            noise_mm: 1.1,
            ..Plan::default()
        },
        Plan {
            noise_mm: f64::NAN,
            ..Plan::default()
        },
        Plan {
            spread_mm: 2.1,
            ..Plan::default()
        },
        Plan {
            spread_mm: -1.0,
            ..Plan::default()
        },
    ] {
        assert!(nozzlez::simulate(&plan).is_err(), "{plan:?}");
    }
    assert!(nozzlez::simulate(&Plan {
        nozzles: 64,
        probes: 1000,
        ..Plan::default()
    })
    .is_ok());
}

#[test]
fn the_probings_that_are_written_are_read_back_to_the_digits_written() {
    for seed in 0..40u64 {
        let made = nozzlez::simulate(&Plan {
            nozzles: 3,
            probes: 4,
            seed,
            ..Plan::default()
        })
        .unwrap();
        let back = nozzlez::parse_readings(&nozzlez::readings_csv(&made.readings)).unwrap();
        assert_eq!(back.len(), made.readings.len(), "seed {seed}");
        for (a, b) in back.iter().zip(&made.readings) {
            assert_eq!(a.nozzle, b.nozzle);
            assert!((a.z_mm - b.z_mm).abs() < 5.1e-5, "seed {seed}");
        }
    }
}

#[test]
fn nozzle_zcal_reads_a_file_or_makes_up_the_probings() {
    let o = run(&["nozzle-zcal", "examples/z_probe.csv"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        out(&o).contains("N1: 4 probings, height 0.0127 mm"),
        "{}",
        out(&o)
    );
    assert!(err(&o).contains("2 nozzles need a look"));
    let o = run(&["nozzle-zcal", "examples/z_probe.csv", "--tol", "0.05"]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("every nozzle is within the tolerance"));
    let o = run(&[
        "nozzle-zcal",
        "--simulate",
        "--nozzles",
        "3",
        "--probes",
        "30",
        "--noise",
        "0.001",
        "--seed",
        "7",
        "--tol",
        "0.5",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(
        out(&o).contains("simulated probings of 3 nozzles, 30 each (seed 7)"),
        "{}",
        out(&o)
    );
    assert!(out(&o).contains("no machine was touched"));
    // the same options give the same text
    let again = run(&[
        "nozzle-zcal",
        "--simulate",
        "--nozzles",
        "3",
        "--probes",
        "30",
        "--noise",
        "0.001",
        "--seed",
        "7",
        "--tol",
        "0.5",
    ]);
    assert_eq!(out(&o), out(&again));
    // the tolerance and the tips come from the settings when they are not given
    let dir = temp("zcal");
    let cfg = dir.join("machine.cfg");
    std::fs::write(&cfg, "nozzles = 4\nnozzle_z_tol_mm = 0.5\nsim_seed = 3\n").unwrap();
    let o = run(&["nozzle-zcal", "--simulate", "--config", p(&cfg)]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).starts_with("4 nozzles"), "{}", out(&o));
    assert!(out(&o).contains("(seed 3)"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn nozzle_zcal_writes_its_tables_once_and_checks_what_it_is_given() {
    let dir = temp("zcalout");
    let table = dir.join("zcal.csv");
    let probes = dir.join("probes.csv");
    let o = run(&[
        "nozzle-zcal",
        "--simulate",
        "--tol",
        "0.5",
        "--out",
        p(&table),
        "--readings-out",
        p(&probes),
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(std::fs::read_to_string(&table)
        .unwrap()
        .starts_with("nozzle,probes,mean_mm,"));
    // the probings that were made up are the file the command reads
    let o = run(&["nozzle-zcal", p(&probes), "--tol", "0.5"]);
    assert!(o.status.success(), "{}", err(&o));
    // never over a file that is there
    let o = run(&["nozzle-zcal", "--simulate", "--out", p(&table)]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("PHOENIX-PNP-040"), "{}", err(&o));
    for bad in [
        vec!["nozzle-zcal"],
        vec!["nozzle-zcal", "--simulate", "examples/z_probe.csv"],
        vec![
            "nozzle-zcal",
            "examples/z_probe.csv",
            "--readings-out",
            "x.csv",
        ],
        vec!["nozzle-zcal", "--simulate", "--nozzles", "0"],
        vec!["nozzle-zcal", "--simulate", "--nozzles", "1.5"],
        vec!["nozzle-zcal", "--simulate", "--probes", "1001"],
        vec!["nozzle-zcal", "--simulate", "--noise", "2"],
        vec!["nozzle-zcal", "--simulate", "--spread", "-1"],
        vec!["nozzle-zcal", "--simulate", "--tol", "0"],
        vec!["nozzle-zcal", "--simulate", "--seed", "-1"],
        vec!["nozzle-zcal", "--simulate", "--what"],
        vec!["nozzle-zcal", "no_such_file.csv"],
    ] {
        let o = run(&bad);
        assert_eq!(o.status.code(), Some(1), "{bad:?}");
        assert!(err(&o).starts_with("error: "), "{bad:?}: {}", err(&o));
    }
    assert!(!root().join("x.csv").exists());
    let _ = std::fs::remove_dir_all(dir);
}

// ---- the marks of the boards of a panel

fn mark(reference: &str, x: f64, y: f64) -> Part {
    Part {
        reference: reference.to_string(),
        value: "Fiducial".to_string(),
        package: "Fiducial".to_string(),
        x,
        y,
        rotation: 0.0,
        side: Side::Top,
    }
}

fn part(reference: &str, x: f64, y: f64) -> Part {
    Part {
        reference: reference.to_string(),
        value: "10k".to_string(),
        package: "R_0603".to_string(),
        x,
        y,
        rotation: 0.0,
        side: Side::Top,
    }
}

fn words() -> (Vec<String>, Vec<String>) {
    (
        vec!["fiducial".to_string()],
        vec!["fid".to_string(), "fd".to_string()],
    )
}

#[test]
fn a_board_is_aligned_by_its_own_marks_when_it_has_two_far_enough_apart() {
    let (w, r) = words();
    let parts = [
        part("A/R1", 5.0, 5.0),
        mark("A/FID1", 0.0, 0.0),
        mark("A/FID2", 30.0, 40.0),
        part("B/R1", 105.0, 5.0),
        mark("B/FID1", 100.0, 0.0),
        part("C/R1", 5.0, 105.0),
        mark("C/FID1", 0.0, 100.0),
        mark("C/FID2", 3.0, 104.0),
        part("D/R1", 5.0, 205.0),
        part("E/R1", 5.0, 305.0),
        mark("E/FID1", 0.0, 300.0),
        mark("E/FID2", 10.0, 300.0),
    ];
    let report = panelfids::analyse(&parts, &["F".to_string()], &w, &r, 10.0);
    let status: Vec<(&str, FStatus)> = report
        .boards
        .iter()
        .map(|b| (b.board.as_str(), b.status))
        .collect();
    assert_eq!(
        status,
        [
            ("A", FStatus::Ok),
            ("B", FStatus::OneMark),
            ("C", FStatus::TooClose),
            ("D", FStatus::NoMark),
            ("E", FStatus::Ok),
        ]
    );
    assert!((report.boards[0].widest_mm - 50.0).abs() < 1e-12);
    assert_eq!(report.problems(), 3);
    assert_eq!(report.left_out, ["F"]);
    // the least distance is the edge: exactly 10 mm apart is enough
    assert_eq!(report.boards[4].status, FStatus::Ok);
    let text = report.text();
    assert!(text.contains("A: 2 marks, the farthest 50.000 mm apart - ok: A/FID1 (0.000, 0.000), A/FID2 (30.000, 40.000)"), "{text}");
    assert!(
        text.contains("D: 0 marks, the farthest 0.000 mm apart - no_mark\n"),
        "{text}"
    );
    assert!(text.contains("F: marked as bad and left out"), "{text}");
    assert!(
        text.ends_with("3 boards cannot be aligned by their own marks\n"),
        "{text}"
    );
    let csv = report.csv();
    assert!(
        csv.starts_with("board,mark,x,y,status\nA,A/FID1,0.0000,0.0000,ok\n"),
        "{csv}"
    );
    assert!(csv.contains("D,,,,no_mark\n"), "{csv}");
    // a mark is told by its reference, whatever the board it is on
    let nested = [mark("B/C/FD1", 0.0, 0.0), mark("B/C/FD2", 20.0, 0.0)];
    let report = panelfids::analyse(&nested, &[], &w, &r, 10.0);
    assert_eq!(
        (report.boards[0].board.as_str(), report.problems()),
        ("B/C", 0)
    );
}

fn load_example(file: &str) -> Result<Vec<Part>, String> {
    let text =
        std::fs::read_to_string(root().join("examples").join(file)).map_err(|e| e.to_string())?;
    Ok(parse_all(&text, &MachineConfig::default().profile)?.parts)
}

#[test]
fn the_marks_of_a_board_are_put_where_the_board_is_in_the_panel() {
    let panel = Panel::parse(&example("panel_marks.json")).unwrap();
    let cfg = MachineConfig::default();
    let report = panelfids::from_panel(&panel, &load_example, &[], &cfg).unwrap();
    // C is marked as bad in the file
    assert_eq!(report.left_out, ["C"]);
    let names: Vec<&str> = report.boards.iter().map(|b| b.board.as_str()).collect();
    assert_eq!(names, ["A", "B"]);
    let a = &report.boards[0].marks;
    assert_eq!((a[0].1, a[0].2, a[1].1, a[1].2), (2.0, 2.0, 48.0, 2.0));
    // B is turned a quarter and moved to X 120: a mark at (2, 2) of the board is at (118, 2) of the panel
    let b = &report.boards[1].marks;
    let near = |got: f64, want: f64| (got - want).abs() < 1e-9;
    assert!(near(b[0].1, 118.0) && near(b[0].2, 2.0), "{b:?}");
    assert!(near(b[1].1, 118.0) && near(b[1].2, 48.0), "{b:?}");
    assert_eq!(report.problems(), 0);
    // more boards left out by the command line; a name no board has is an error; all of them is one too
    let report = panelfids::from_panel(&panel, &load_example, &["A".to_string()], &cfg).unwrap();
    assert_eq!(report.boards.len(), 1);
    assert_eq!(report.left_out.len(), 2);
    assert!(panelfids::from_panel(&panel, &load_example, &["Z".to_string()], &cfg).is_err());
    assert!(panelfids::from_panel(
        &panel,
        &load_example,
        &["A".to_string(), "B".to_string()],
        &cfg
    )
    .is_err());
}

#[test]
fn panel_fids_lists_the_marks_and_fails_when_a_board_cannot_be_aligned() {
    let o = run(&["panel-fids", "examples/panel_marks.json"]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("B: 2 marks, the farthest 46.000 mm apart - ok: B/FID1 (118.000, 2.000), B/FID2 (118.000, 48.000)"), "{}", out(&o));
    assert!(out(&o).contains("C: marked as bad and left out"));
    // the least distance is a setting and an option
    let o = run(&["panel-fids", "examples/panel_marks.json", "--min-sep", "50"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(out(&o).contains("- too_close"), "{}", out(&o));
    assert!(
        err(&o).contains("2 boards cannot be aligned"),
        "{}",
        err(&o)
    );
    let dir = temp("fidscfg");
    let cfg = dir.join("machine.cfg");
    std::fs::write(&cfg, "panel_fid_min_sep_mm = 100\n").unwrap();
    let o = run(&[
        "panel-fids",
        "examples/panel_marks.json",
        "--config",
        p(&cfg),
    ]);
    assert_eq!(o.status.code(), Some(2));
    // the board of the plain example has no mark
    let o = run(&["panel-fids", "examples/panel.json"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(out(&o).contains("- no_mark"), "{}", out(&o));
    // a board marked as bad on the command line
    let o = run(&["panel-fids", "examples/panel_marks.json", "--x-out", "A"]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("A: marked as bad and left out"));
    let table = dir.join("fids.csv");
    let o = run(&[
        "panel-fids",
        "examples/panel_marks.json",
        "--out",
        p(&table),
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert_eq!(std::fs::read_to_string(&table).unwrap().lines().count(), 5);
    let o = run(&[
        "panel-fids",
        "examples/panel_marks.json",
        "--out",
        p(&table),
    ]);
    assert_eq!(o.status.code(), Some(1));
    for bad in [
        vec!["panel-fids"],
        vec!["panel-fids", "examples/panel_marks.json", "--min-sep", "-1"],
        vec![
            "panel-fids",
            "examples/panel_marks.json",
            "--min-sep",
            "1001",
        ],
        vec!["panel-fids", "examples/panel_marks.json", "--x-out", "Z"],
        vec!["panel-fids", "examples/nothing.json"],
        vec!["panel-fids", "examples/board.pos"],
        vec!["panel-fids", "examples/panel_marks.json", "extra"],
    ] {
        let o = run(&bad);
        assert_eq!(o.status.code(), Some(1), "{bad:?}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

// ---- the tapes, step by step

fn feeder(id: &str, count: u32) -> FeederSpec {
    FeederSpec::from_basic(Feeder {
        id: id.to_string(),
        value: "10k".to_string(),
        package: "R_0603".to_string(),
        x: 0.0,
        y: 0.0,
        count,
    })
}

fn limits() -> Limits {
    Limits {
        x_min: -100.0,
        x_max: 100.0,
        y_min: -100.0,
        y_max: 100.0,
        pitch_tol_mm: 0.05,
    }
}

#[test]
fn every_pick_says_which_part_of_the_reel_it_is_and_how_many_are_left() {
    let feeders = [feeder("F1", 5)];
    let placements: Vec<(&str, &str, (f64, f64))> =
        (1..=5).map(|i| ("R", "F1", (0.0, f64::from(i)))).collect();
    let steps = tapesteps::walk(&placements, &feeders, &limits());
    let left: Vec<i64> = steps.iter().map(|s| s.remaining).collect();
    assert_eq!(left, [4, 3, 2, 1, 0]);
    assert_eq!(
        steps.iter().map(|s| s.taken).collect::<Vec<_>>(),
        [1, 2, 3, 4, 5]
    );
    assert_eq!(steps.last().unwrap().status, TStatus::Last);
    assert!(steps[..4].iter().all(|s| s.status == TStatus::Ok));
    assert_eq!(steps[0].advance_mm, None);
    assert_eq!(steps[1].advance_mm, Some(1.0));
    assert_eq!(tapesteps::faults(&steps), 0);
}

#[test]
fn a_reel_that_runs_dry_is_a_fault_from_the_first_pick_with_nothing_left() {
    let feeders = [feeder("F1", 2)];
    let placements: Vec<(&str, &str, (f64, f64))> = vec![
        ("R1", "F1", (0.0, 0.0)),
        ("R2", "F1", (0.0, 0.0)),
        ("R3", "F1", (0.0, 0.0)),
        ("R4", "F1", (0.0, 0.0)),
    ];
    let steps = tapesteps::walk(&placements, &feeders, &limits());
    let status: Vec<TStatus> = steps.iter().map(|s| s.status).collect();
    assert_eq!(
        status,
        [TStatus::Ok, TStatus::Last, TStatus::Empty, TStatus::Empty]
    );
    assert_eq!(steps[3].remaining, -2);
    assert_eq!(tapesteps::faults(&steps), 2);
    let text = tapesteps::text(&steps);
    assert!(
        text.starts_with(
            "4 steps on 1 feeders, 2 faults\nF1: 4 picks, -2 parts left after the last\n"
        ),
        "{text}"
    );
    assert!(
        text.contains(
            "step 3 (R3): F1, part 3 of the reel, -1 left, picked at 0.000,0.000 - empty"
        ),
        "{text}"
    );
}

#[test]
fn the_warning_of_a_feeder_is_given_once_at_the_first_step_under_it() {
    let mut f = feeder("F1", 6);
    f.warn_below = 3;
    let placements: Vec<(&str, &str, (f64, f64))> =
        (0..6).map(|_| ("R", "F1", (0.0, 0.0))).collect();
    let status: Vec<TStatus> = tapesteps::walk(&placements, &[f], &limits())
        .iter()
        .map(|s| s.status)
        .collect();
    // 5, 4, 3 left are fine, 2 left is the first under 3, then the rest, and the last part is the last
    assert_eq!(
        status,
        [
            TStatus::Ok,
            TStatus::Ok,
            TStatus::Ok,
            TStatus::Low,
            TStatus::Ok,
            TStatus::Last
        ]
    );
    // no warning set: no step is low
    let f = feeder("F1", 6);
    assert!(tapesteps::walk(&placements, &[f], &limits())
        .iter()
        .all(|s| s.status != TStatus::Low));
}

#[test]
fn a_strip_must_advance_by_its_pitch_and_the_place_must_be_in_reach() {
    let mut strip = feeder("S", 10);
    strip.kind = FeederKind::Strip {
        pitch: 4.0,
        rotation: 0.0,
    };
    let placements: Vec<(&str, &str, (f64, f64))> = vec![
        ("R1", "S", (10.0, 0.0)),
        ("R2", "S", (14.0, 0.0)),
        ("R3", "S", (18.03, 0.0)),
        ("R4", "S", (23.0, 0.0)),
        ("R5", "S", (150.0, 0.0)),
    ];
    let steps = tapesteps::walk(&placements, std::slice::from_ref(&strip), &limits());
    let status: Vec<TStatus> = steps.iter().map(|s| s.status).collect();
    // 4.03 is within the tolerance of the pitch, 4.97 is not, and the last one is out of reach
    assert_eq!(
        status,
        [
            TStatus::Ok,
            TStatus::Ok,
            TStatus::Ok,
            TStatus::Pitch,
            TStatus::Reach
        ]
    );
    assert!((steps[3].advance_mm.unwrap() - 4.97).abs() < 1e-9);
    // a tape that is not a strip has no pitch to keep
    let tape = feeder("T", 10);
    let moved: Vec<(&str, &str, (f64, f64))> =
        vec![("R1", "T", (0.0, 0.0)), ("R2", "T", (3.0, 0.0))];
    assert!(tapesteps::walk(&moved, &[tape], &limits())
        .iter()
        .all(|s| s.status == TStatus::Ok));
    // a pitch of the other direction is the same pitch
    let mut back = feeder("B", 10);
    back.kind = FeederKind::Strip {
        pitch: -4.0,
        rotation: 0.0,
    };
    let reverse: Vec<(&str, &str, (f64, f64))> =
        vec![("R1", "B", (10.0, 0.0)), ("R2", "B", (6.0, 0.0))];
    assert_eq!(
        tapesteps::faults(&tapesteps::walk(&reverse, &[back], &limits())),
        0
    );
    // an edge of the area is inside it
    let edge: Vec<(&str, &str, (f64, f64))> = vec![("R1", "T", (100.0, -100.0))];
    assert_eq!(
        tapesteps::faults(&tapesteps::walk(&edge, &[feeder("T", 1)], &limits())),
        0
    );
}

#[test]
fn a_feeder_the_table_does_not_have_is_told_and_the_steps_keep_the_order_of_the_plan() {
    let placements: Vec<(&str, &str, (f64, f64))> = vec![
        ("R1", "F1", (0.0, 0.0)),
        ("R2", "GHOST", (0.0, 0.0)),
        ("R3", "F1", (0.0, 0.0)),
    ];
    let steps = tapesteps::walk(&placements, &[feeder("F1", 9)], &limits());
    assert_eq!(steps[1].status, TStatus::NoFeeder);
    assert_eq!(steps.iter().map(|s| s.step).collect::<Vec<_>>(), [1, 2, 3]);
    assert_eq!((steps[2].taken, steps[2].remaining), (2, 7));
    assert_eq!(tapesteps::faults(&steps), 1);
    let csv = tapesteps::csv(&steps);
    assert!(csv.starts_with("step,reference,feeder,taken,remaining,pick_x,pick_y,advance_mm,status\n1,R1,F1,1,8,0.000,0.000,,ok\n"), "{csv}");
    assert!(
        csv.contains("2,R2,GHOST,1,0,0.000,0.000,,nofeeder\n"),
        "{csv}"
    );
    assert!(tapesteps::walk(&[], &[], &limits()).is_empty());
}

#[test]
fn tape_steps_walks_the_plan_of_the_tables_and_fails_on_a_reel_that_runs_dry() {
    let o = run(&["tape-steps", "examples/board.pos", "examples/feeders.csv"]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(
        out(&o).starts_with("6 steps on 3 feeders, 0 faults\n"),
        "{}",
        out(&o)
    );
    let dir = temp("tapes");
    let short = dir.join("feeders.csv");
    std::fs::write(&short, "id,value,package,x,y,count,warn_below\nF1,10k,R_0603,-20,10,3,2\nF2,100n,C_0603,-20,20,100,0\nF3,MCU,QFN-32,-20,40,10,0\nF4,LED,LED_0603,-20,50,50,0\n").unwrap();
    let table = dir.join("steps.csv");
    // a reel of three for three picks: the second leaves one (under the warning of two) and the third takes the last
    let o = run(&[
        "tape-steps",
        "examples/board.pos",
        p(&short),
        "--out",
        p(&table),
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(
        out(&o).contains("part 2 of the reel, 1 left, picked at -20.000,10.000 - low"),
        "{}",
        out(&o)
    );
    assert!(
        out(&o).contains("part 3 of the reel, 0 left, picked at -20.000,10.000 - last"),
        "{}",
        out(&o)
    );
    let csv = std::fs::read_to_string(&table).unwrap();
    assert_eq!(csv.lines().count(), 7);
    assert!(
        csv.lines().any(|l| l.ends_with(",low")) && csv.lines().any(|l| l.ends_with(",last")),
        "{csv}"
    );
    // never over a file that is there or one that was read
    let o = run(&[
        "tape-steps",
        "examples/board.pos",
        p(&short),
        "--out",
        p(&table),
    ]);
    assert_eq!(o.status.code(), Some(1));
    let o = run(&[
        "tape-steps",
        "examples/board.pos",
        p(&short),
        "--out",
        p(&short),
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("also an input"), "{}", err(&o));
    // the area the machine reaches comes from the settings
    let cfg = dir.join("machine.cfg");
    std::fs::write(&cfg, "limit_x_min_mm = -10\n").unwrap();
    let o = run(&[
        "tape-steps",
        "examples/board.pos",
        "examples/feeders.csv",
        "--config",
        p(&cfg),
    ]);
    assert_eq!(o.status.code(), Some(2));
    assert!(err(&o).contains("use --partial"), "{}", err(&o));
    let o = run(&[
        "tape-steps",
        "examples/board.pos",
        "examples/feeders.csv",
        "--config",
        p(&cfg),
        "--partial",
    ]);
    assert_eq!(o.status.code(), Some(2));
    assert!(out(&o).contains("- reach"), "{}", out(&o));
    assert!(
        err(&o).contains("steps are a fault of the tapes"),
        "{}",
        err(&o)
    );
    for bad in [
        vec!["tape-steps"],
        vec!["tape-steps", "examples/board.pos"],
        vec!["tape-steps", "examples/board.pos", "no_such.csv"],
        vec![
            "tape-steps",
            "examples/board.pos",
            "examples/feeders.csv",
            "--what",
        ],
    ] {
        assert_eq!(run(&bad).status.code(), Some(1), "{bad:?}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

// ---- the effectiveness of a shift

fn runs_of(seconds: &[f64], complete: &[bool]) -> Runs {
    Runs {
        parts: 7,
        rows: seconds
            .iter()
            .zip(complete)
            .enumerate()
            .map(|(i, (s, c))| Run {
                seed: i as u64,
                phase: Phase::Finished,
                placed: if *c { 7 } else { 6 },
                deferred: usize::from(!*c),
                skipped: 0,
                pending: 0,
                seconds: *s,
                error: None,
            })
            .collect(),
    }
}

fn changes(boards: u64, reels: &[(u32, u32)]) -> u64 {
    reels
        .iter()
        .map(|(per, count)| {
            (boards * u64::from(*per))
                .div_ceil(u64::from(*count))
                .saturating_sub(1)
        })
        .sum()
}

#[test]
fn the_effectiveness_is_the_product_of_availability_performance_and_quality() {
    let inputs = Inputs {
        ideal_s: 8.0,
        runs: runs_of(&[10.0, 10.0, 10.0, 10.0], &[true, true, true, false]),
        reels: vec![(3, 100), (1, 40)],
        shift_s: 3600.0,
        reel_change_s: 60.0,
    };
    let oee = shiftoee::compute(&inputs).unwrap();
    assert_eq!(oee.runs, 4);
    assert!((oee.actual_s - 10.0).abs() < 1e-12);
    assert!((oee.performance - 0.8).abs() < 1e-12);
    assert!((oee.quality - 0.75).abs() < 1e-12);
    // the boards are the most that fit counting the stops
    let used = |n: u64| n as f64 * 10.0 + changes(n, &inputs.reels) as f64 * 60.0;
    assert!(
        used(oee.boards) <= 3600.0 && used(oee.boards + 1) > 3600.0,
        "{}",
        oee.boards
    );
    assert_eq!(oee.reel_changes, changes(oee.boards, &inputs.reels));
    assert!((oee.stopped_s - oee.reel_changes as f64 * 60.0).abs() < 1e-9);
    assert!((oee.availability - (3600.0 - oee.stopped_s) / 3600.0).abs() < 1e-12);
    assert!((oee.oee - oee.availability * oee.performance * oee.quality).abs() < 1e-12);
    assert!((oee.good_boards - oee.boards as f64 * 0.75).abs() < 1e-9);
    for share in [oee.availability, oee.performance, oee.quality, oee.oee] {
        assert!((0.0..=1.0).contains(&share));
    }
}

#[test]
fn a_shift_without_reel_changes_is_all_available_and_a_faster_run_than_the_plan_does_not_beat_100()
{
    let inputs = Inputs {
        ideal_s: 20.0,
        runs: runs_of(&[10.0, 10.0], &[true, true]),
        reels: vec![],
        shift_s: 100.0,
        reel_change_s: 60.0,
    };
    let oee = shiftoee::compute(&inputs).unwrap();
    assert_eq!((oee.boards, oee.reel_changes), (10, 0));
    assert_eq!(
        (oee.availability, oee.performance, oee.quality, oee.oee),
        (1.0, 1.0, 1.0, 1.0)
    );
    // a reel of one board needs a change for every board after the first
    let inputs = Inputs {
        reels: vec![(1, 1)],
        ..inputs
    };
    let oee = shiftoee::compute(&inputs).unwrap();
    assert!(oee.reel_changes + 1 == oee.boards, "{oee:?}");
    // feeders that nothing takes from, or with no parts, do not stop the shift
    let inputs = Inputs {
        reels: vec![(0, 5), (3, 0)],
        ..inputs
    };
    assert_eq!(shiftoee::compute(&inputs).unwrap().reel_changes, 0);
}

#[test]
fn the_shift_is_refused_when_there_is_nothing_to_measure() {
    let good = Inputs {
        ideal_s: 8.0,
        runs: runs_of(&[10.0], &[true]),
        reels: vec![(1, 10)],
        shift_s: 600.0,
        reel_change_s: 10.0,
    };
    assert!(shiftoee::compute(&good).is_ok());
    for bad in [
        Inputs {
            shift_s: 0.0,
            ..good.clone()
        },
        Inputs {
            shift_s: f64::NAN,
            ..good.clone()
        },
        Inputs {
            reel_change_s: -1.0,
            ..good.clone()
        },
        Inputs {
            ideal_s: 0.0,
            ..good.clone()
        },
        Inputs {
            runs: runs_of(&[0.0], &[true]),
            ..good.clone()
        },
    ] {
        assert!(shiftoee::compute(&bad).is_err(), "{bad:?}");
    }
    let mut stopped = runs_of(&[10.0, 10.0], &[true, true]);
    for r in &mut stopped.rows {
        r.pending = 3;
        r.error = Some("PHOENIX-PNP-022".to_string());
    }
    let e = shiftoee::compute(&Inputs {
        runs: stopped,
        ..good.clone()
    })
    .unwrap_err();
    assert!(e.to_string().contains("no run went to the end"), "{e}");
    // a shift too short for one board has none
    let tiny = Inputs {
        shift_s: 5.0,
        ..good
    };
    assert_eq!(shiftoee::compute(&tiny).unwrap().boards, 0);
}

#[test]
fn the_report_and_the_table_of_the_shift_are_the_same_figures() {
    let inputs = Inputs {
        ideal_s: 8.0,
        runs: runs_of(&[10.0, 10.0], &[true, true]),
        reels: vec![],
        shift_s: 100.0,
        reel_change_s: 0.0,
    };
    let oee = shiftoee::compute(&inputs).unwrap();
    let text = oee.text(480);
    assert!(
        text.starts_with("a shift of 480 minutes, from 2 runs on the simulated machine\n"),
        "{text}"
    );
    assert!(text.contains("a board takes 8.0 s in the plan and 10.0 s in the runs; 10 boards fit, about 10.0 of them good"), "{text}");
    assert!(
        text.ends_with(
            "availability 100.0%, performance 80.0%, quality 100.0%: effectiveness 80.0%\n"
        ),
        "{text}"
    );
    let csv = oee.csv();
    assert!(
        csv.starts_with("metric,value\nruns,2\nboards,10\n"),
        "{csv}"
    );
    assert!(
        csv.ends_with("performance,0.8000\nquality,1.0000\noee,0.8000\n"),
        "{csv}"
    );
}

#[test]
fn shift_oee_runs_the_plan_on_the_simulated_machine_and_gives_the_same_figures_for_the_same_seed() {
    let o = run(&[
        "shift-oee",
        "examples/board.pos",
        "examples/feeders.csv",
        "--runs",
        "8",
        "--seed",
        "5",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(
        out(&o).starts_with("a shift of 480 minutes, from 8 runs on the simulated machine\n"),
        "{}",
        out(&o)
    );
    assert!(out(&o).contains("effectiveness"));
    let again = run(&[
        "shift-oee",
        "examples/board.pos",
        "examples/feeders.csv",
        "--runs",
        "8",
        "--seed",
        "5",
    ]);
    assert_eq!(out(&o), out(&again));
    // a shorter shift and slower changes of reels make fewer boards
    let short = run(&[
        "shift-oee",
        "examples/board.pos",
        "examples/feeders.csv",
        "--runs",
        "8",
        "--seed",
        "5",
        "--shift",
        "60",
        "--reel-change",
        "300",
    ]);
    assert!(short.status.success(), "{}", err(&short));
    assert!(
        out(&short).starts_with("a shift of 60 minutes"),
        "{}",
        out(&short)
    );
    let boards = |t: &str| -> u64 {
        let line = t.lines().find(|l| l.contains("boards fit")).unwrap();
        line.split("; ")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    assert!(boards(&out(&short)) < boards(&out(&o)));
    // the settings give the shift and the change
    let dir = temp("oee");
    let cfg = dir.join("machine.cfg");
    std::fs::write(
        &cfg,
        "shift_minutes = 120\nreel_change_s = 0\nsimulate_runs_count = 3\n",
    )
    .unwrap();
    let o = run(&[
        "shift-oee",
        "examples/board.pos",
        "examples/feeders.csv",
        "--config",
        p(&cfg),
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(
        out(&o).starts_with("a shift of 120 minutes, from 3 runs"),
        "{}",
        out(&o)
    );
    assert!(out(&o).contains("0 changes of reels"), "{}", out(&o));
    let table = dir.join("oee.csv");
    let o = run(&[
        "shift-oee",
        "examples/board.pos",
        "examples/feeders.csv",
        "--runs",
        "3",
        "--out",
        p(&table),
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(std::fs::read_to_string(&table)
        .unwrap()
        .starts_with("metric,value\nruns,3\n"));
    assert_eq!(
        run(&[
            "shift-oee",
            "examples/board.pos",
            "examples/feeders.csv",
            "--out",
            p(&table)
        ])
        .status
        .code(),
        Some(1)
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn shift_oee_fails_under_the_effectiveness_that_is_asked_and_checks_its_options() {
    let o = run(&[
        "shift-oee",
        "examples/board.pos",
        "examples/feeders.csv",
        "--runs",
        "4",
        "--min-oee",
        "100",
    ]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        err(&o).contains("under the 100% that was asked"),
        "{}",
        err(&o)
    );
    for bad in [
        vec![
            "shift-oee",
            "examples/board.pos",
            "examples/feeders.csv",
            "--runs",
            "0",
        ],
        vec![
            "shift-oee",
            "examples/board.pos",
            "examples/feeders.csv",
            "--runs",
            "10001",
        ],
        vec![
            "shift-oee",
            "examples/board.pos",
            "examples/feeders.csv",
            "--seed",
            "x",
        ],
        vec![
            "shift-oee",
            "examples/board.pos",
            "examples/feeders.csv",
            "--shift",
            "29",
        ],
        vec![
            "shift-oee",
            "examples/board.pos",
            "examples/feeders.csv",
            "--shift",
            "1441",
        ],
        vec![
            "shift-oee",
            "examples/board.pos",
            "examples/feeders.csv",
            "--shift",
            "60.5",
        ],
        vec![
            "shift-oee",
            "examples/board.pos",
            "examples/feeders.csv",
            "--reel-change",
            "-1",
        ],
        vec![
            "shift-oee",
            "examples/board.pos",
            "examples/feeders.csv",
            "--reel-change",
            "3601",
        ],
        vec![
            "shift-oee",
            "examples/board.pos",
            "examples/feeders.csv",
            "--min-oee",
            "101",
        ],
        vec!["shift-oee", "examples/board.pos"],
    ] {
        let o = run(&bad);
        assert_eq!(o.status.code(), Some(1), "{bad:?}");
        assert!(err(&o).starts_with("error: "), "{bad:?}");
    }
}

// ---- the position files of assemblers

fn board(rng: &mut Rng, n: usize) -> Vec<Part> {
    let kinds = [
        ("10k", "R_0603"),
        ("100n", "C_0603"),
        ("MCU", "QFN-32"),
        ("LED", "LED_0603"),
    ];
    (0..n)
        .map(|i| {
            let (value, package) = kinds[below(rng, kinds.len())];
            Part {
                reference: format!("X{}", i + 1),
                value: value.to_string(),
                package: package.to_string(),
                x: below(rng, 200_000) as f64 / 1000.0 - 50.0,
                y: below(rng, 200_000) as f64 / 1000.0 - 50.0,
                rotation: below(rng, 36_000) as f64 / 100.0,
                side: if below(rng, 4) == 0 {
                    Side::Bottom
                } else {
                    Side::Top
                },
            }
        })
        .collect()
}

fn same_parts(a: &[Part], b: &[Part], place_tol: f64, seed: u64) {
    assert_eq!(a.len(), b.len(), "seed {seed}");
    let mut b: Vec<&Part> = b.iter().collect();
    b.sort_by(|x, y| x.reference.cmp(&y.reference));
    let mut a: Vec<&Part> = a.iter().collect();
    a.sort_by(|x, y| x.reference.cmp(&y.reference));
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(
            (&x.reference, &x.value, &x.package, x.side),
            (&y.reference, &y.value, &y.package, y.side),
            "seed {seed}"
        );
        assert!(
            (x.x - y.x).abs() <= place_tol && (x.y - y.y).abs() <= place_tol,
            "seed {seed}: {x:?} {y:?}"
        );
        let turn = (x.rotation - y.rotation).abs();
        assert!(turn.min(360.0 - turn) <= 0.0051, "seed {seed}: {x:?} {y:?}");
    }
}

#[test]
fn what_export_assembler_writes_is_read_back_for_boards_made_by_chance() {
    for seed in 0..80u64 {
        let mut rng = Rng::new(seed);
        let n = 1 + below(&mut rng, 40);
        let parts = board(&mut rng, n);
        for (unit, tol) in [(Unit::Mm, 0.0006), (Unit::Inch, 0.0006)] {
            let text = write(&parts, Format::Cpl, unit, Sides::All);
            let back =
                asmimport::parse(&text).unwrap_or_else(|e| panic!("seed {seed}: {e}\n{text}"));
            assert_eq!(back.dialect, "cpl", "seed {seed}");
            same_parts(&parts, &back.parts, tol, seed);
        }
        let text = write(&parts, Format::Generic, Unit::Mm, Sides::All);
        let back = asmimport::parse(&text).unwrap();
        assert_eq!(back.dialect, "generic");
        same_parts(&parts, &back.parts, 0.0006, seed);
    }
}

#[test]
fn an_altium_file_with_places_in_mils_and_layers_in_words_is_read() {
    let found = asmimport::parse(&example("altium_pnp.csv")).unwrap();
    assert_eq!(found.dialect, "altium");
    let want = parse_all(&example("board.pos"), &MachineConfig::default().profile)
        .unwrap()
        .parts;
    same_parts(&want, &found.parts, 0.0006, 0);
    assert_eq!(
        found
            .parts
            .iter()
            .filter(|p| p.side == Side::Bottom)
            .count(),
        1
    );
    // the unit is in the number, or in the name of the column, or it is millimetres
    let text = "Designator,Mid X,Mid Y,Rotation,Layer\nA,1in,490mil,-90,T\nB,12.5mm,3,450,Bottom\nC,1e1,2 mm,0deg,top layer\n";
    let found = asmimport::parse(text).unwrap();
    assert_eq!(found.dialect, "cpl");
    let p = &found.parts;
    assert!(
        (p[0].x - 25.4).abs() < 1e-9
            && (p[0].y - 12.446).abs() < 1e-9
            && (p[0].rotation - 270.0).abs() < 1e-9
    );
    assert!(
        (p[1].x - 12.5).abs() < 1e-9
            && p[1].y == 3.0
            && (p[1].rotation - 90.0).abs() < 1e-9
            && p[1].side == Side::Bottom
    );
    assert!((p[2].x - 10.0).abs() < 1e-9 && p[2].side == Side::Top);
    let found = asmimport::parse("ref;x(in);y(in);side\nA;1;2;f\n").unwrap();
    assert_eq!((found.parts[0].x, found.parts[0].y), (25.4, 50.8));
    assert_eq!(found.dialect, "generic");
    assert_eq!(
        asmimport::parse("Designator,X,Y,Layer\nA,1,2,top\n")
            .unwrap()
            .dialect,
        "table"
    );
}

#[test]
fn a_position_file_that_cannot_be_read_is_an_error_with_its_line() {
    for (text, why) in [
        ("", "the file is empty"),
        ("Designator,Mid X,Mid Y\nA,1,2\n", "no column of the layer"),
        ("Mid X,Mid Y,Layer\n1,2,top\n", "no column of designators"),
        (
            "Designator,Mid Y,Layer\nA,2,top\n",
            "no column of the place along X",
        ),
        (
            "Designator,Mid X,Layer\nA,2,top\n",
            "no column of the place along Y",
        ),
        ("Designator,Mid X,Mid Y,Layer\n", "no parts"),
        (
            "Designator,Mid X,Mid Y,Layer\nA,1,2,top\nA,3,4,top\n",
            "line 3: the designator A is in the file twice",
        ),
        (
            "Designator,Mid X,Mid Y,Layer\n,1,2,top\n",
            "line 2: the designator is empty",
        ),
        (
            "Designator,Mid X,Mid Y,Layer\nA,1cm,2,top\n",
            "line 2: the place along X",
        ),
        (
            "Designator,Mid X,Mid Y,Layer\nA,1,,top\n",
            "line 2: the place along Y",
        ),
        (
            "Designator,Mid X,Mid Y,Layer\nA,1,2,middle\n",
            "line 2: the layer is top or bottom",
        ),
        (
            "Designator,Mid X,Mid Y,Rotation,Layer\nA,1,2,x,top\n",
            "line 2: the rotation",
        ),
        (
            "Designator,Mid X,Mid Y,Layer\nA,NaN,2,top\n",
            "the place along X",
        ),
        ("Designator,Mid X,Mid Y,Layer\n\"A,1,2,top\n", "quote"),
    ] {
        let e = asmimport::parse(text).unwrap_err().to_string();
        assert!(e.contains(why), "{text:?}: {e}");
        assert!(e.contains("PHOENIX-PNP-053"), "{e}");
    }
}

#[test]
fn import_assembler_gives_the_parts_in_a_shape_every_command_reads() {
    let o = run(&["import-assembler", "examples/altium_pnp.csv"]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(
        err(&o).contains("7 parts read from examples/altium_pnp.csv (shape: altium)"),
        "{}",
        err(&o)
    );
    let parsed = parse_all(&out(&o), &MachineConfig::default().profile).unwrap();
    assert_eq!(parsed.parts.len(), 7);
    let dir = temp("asmimport");
    let file = dir.join("board.pos");
    let o = run(&[
        "import-assembler",
        "examples/altium_pnp.csv",
        "--format",
        "kicad-pos",
        "--out",
        p(&file),
    ]);
    assert!(o.status.success(), "{}", err(&o));
    // the table that was written is the board of the example, and the command that reads parts reads it
    let o = run(&["parts", p(&file)]);
    assert!(o.status.success(), "{}", err(&o));
    let o = run(&[
        "import-assembler",
        "examples/altium_pnp.csv",
        "--out",
        p(&file),
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("PHOENIX-PNP-040"), "{}", err(&o));
    let o = run(&["import-assembler", p(&file), "--out", p(&file)]);
    assert_eq!(o.status.code(), Some(1));
    let o = run(&[
        "import-assembler",
        "examples/altium_pnp.csv",
        "--format",
        "cpl",
        "--unit",
        "inch",
    ]);
    assert!(
        out(&o).contains("Designator,Val,Package,Mid X,Mid Y,Rotation,Layer"),
        "{}",
        out(&o)
    );
    assert!(out(&o).contains("0.393701in"), "{}", out(&o));
    for bad in [
        vec!["import-assembler"],
        vec!["import-assembler", "examples/board.pos"],
        vec![
            "import-assembler",
            "examples/altium_pnp.csv",
            "--format",
            "gerber",
        ],
        vec![
            "import-assembler",
            "examples/altium_pnp.csv",
            "--unit",
            "cm",
        ],
        vec!["import-assembler", "no_such_file.csv"],
        vec!["import-assembler", "examples/altium_pnp.csv", "extra"],
    ] {
        assert_eq!(run(&bad).status.code(), Some(1), "{bad:?}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

// ---- the bills of materials of several boards

fn bom(text: &str) -> Bom {
    Bom::parse(text, 1000).unwrap()
}

#[test]
fn the_rows_that_are_one_part_are_joined_and_every_board_is_counted_for_its_boards() {
    let a = Input {
        name: "a".into(),
        boards: 2,
        bom: bom(&example("bom.csv")),
    };
    let b = Input {
        name: "b".into(),
        boards: 3,
        bom: bom(&example("bom_b.csv")),
    };
    let lines = bommerge::merge(&[a.clone(), b.clone()]).unwrap();
    assert_eq!(lines.len(), 4);
    let find = |value: &str| lines.iter().find(|l| l.value == value).unwrap();
    assert_eq!(find("10k").total, 3 * 2 + 2 * 3);
    assert_eq!(
        find("10k").used_by,
        [("a".to_string(), 3, 2), ("b".to_string(), 2, 3)]
    );
    assert_eq!(find("100n").total, 2 * 2 + 3);
    assert_eq!(find("MCU").total, 2 + 3);
    assert_eq!(find("LED").total, 2);
    assert_eq!(find("LED").used_by.len(), 1);
    assert_eq!(find("10k").part_number, "RC0603FR-0710KL");
    // the order is the package, then the value
    let packages: Vec<&str> = lines.iter().map(|l| l.package.as_str()).collect();
    let mut sorted = packages.clone();
    sorted.sort_by_key(|s| s.to_ascii_lowercase());
    assert_eq!(packages, sorted);
    // the same bill alone is its own rows times the boards
    let only = bommerge::merge(&[Input {
        boards: 5,
        ..a.clone()
    }])
    .unwrap();
    assert_eq!(only.iter().map(|l| l.total).sum::<u64>(), 7 * 5);
    // values written alike are one part
    let alike = Input {
        name: "c".into(),
        boards: 1,
        bom: bom("Designator,Comment,Footprint\nR9,10K,R_0603\nR8,10k,r_0603\n"),
    };
    let joined = bommerge::merge(&[alike]).unwrap();
    assert_eq!((joined.len(), joined[0].total), (1, 2));
}

#[test]
fn the_list_to_buy_is_a_table_and_a_report() {
    let a = Input {
        name: "a".into(),
        boards: 2,
        bom: bom(&example("bom.csv")),
    };
    let b = Input {
        name: "b".into(),
        boards: 3,
        bom: bom(&example("bom_b.csv")),
    };
    let inputs = [a, b];
    let lines = bommerge::merge(&inputs).unwrap();
    let csv = bommerge::csv(&lines);
    assert!(
        csv.starts_with("value,package,part_number,total,used_by\n"),
        "{csv}"
    );
    assert!(
        csv.contains("10k,R_0603,RC0603FR-0710KL,12,\"a:3x2;b:2x3\"\n"),
        "{csv}"
    );
    let text = bommerge::text(&lines, &inputs);
    assert!(
        text.starts_with("2 bills for 5 boards: 4 different parts, 26 parts in all\n"),
        "{text}"
    );
    assert!(
        text.contains("10k R_0603 [RC0603FR-0710KL]: 12 (a 3 x 2, b 2 x 3)"),
        "{text}"
    );
}

#[test]
fn bills_that_cannot_be_added_are_refused() {
    let a = Input {
        name: "a".into(),
        boards: 1,
        bom: bom(&example("bom.csv")),
    };
    assert!(bommerge::merge(&[]).is_err());
    assert!(bommerge::merge(&[a.clone(), a.clone()])
        .unwrap_err()
        .to_string()
        .contains("given twice"));
    assert!(bommerge::merge(&[Input {
        boards: 0,
        ..a.clone()
    }])
    .is_err());
    assert!(bommerge::merge(&[Input {
        boards: bommerge::MAX_BOARDS + 1,
        ..a.clone()
    }])
    .is_err());
    let many: Vec<Input> = (0..=bommerge::MAX_BILLS)
        .map(|i| Input {
            name: format!("b{i}"),
            ..a.clone()
        })
        .collect();
    assert!(bommerge::merge(&many).is_err());
    assert!(bommerge::merge(&many[..bommerge::MAX_BILLS]).is_ok());
}

#[test]
fn bom_merge_adds_the_files_named_with_their_boards() {
    let o = run(&["bom-merge", "examples/bom.csv:2", "examples/bom_b.csv:3"]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(
        out(&o).starts_with("2 bills for 5 boards: 4 different parts, 26 parts in all\n"),
        "{}",
        out(&o)
    );
    assert!(out(&o).contains("(bom 3 x 2, bom_b 2 x 3)"), "{}", out(&o));
    let dir = temp("bommerge");
    let table = dir.join("buy.csv");
    let o = run(&[
        "bom-merge",
        "examples/bom.csv",
        "examples/bom_b.csv",
        "--out",
        p(&table),
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(std::fs::read_to_string(&table)
        .unwrap()
        .contains("10k,R_0603,RC0603FR-0710KL,5,\"bom:3x1;bom_b:2x1\"\n"));
    assert_eq!(
        run(&["bom-merge", "examples/bom.csv", "--out", p(&table)])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        run(&["bom-merge", "examples/bom.csv", "--out", "examples/bom.csv"])
            .status
            .code(),
        Some(1)
    );
    for bad in [
        vec!["bom-merge"],
        vec!["bom-merge", "examples/bom.csv:0"],
        vec!["bom-merge", "examples/bom.csv:1000001"],
        vec!["bom-merge", "examples/bom.csv", "examples/bom.csv"],
        vec!["bom-merge", "no_such_bom.csv"],
        vec!["bom-merge", "examples/board.pos"],
        vec!["bom-merge", "examples/bom.csv", "--max-rows", "1"],
        vec!["bom-merge", "examples/bom.csv", "--max-rows", "0"],
    ] {
        let o = run(&bad);
        assert_eq!(o.status.code(), Some(1), "{bad:?}");
        assert!(err(&o).starts_with("error: "), "{bad:?}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

// ---- the new commands never panic on bytes made by chance

#[test]
fn the_new_readers_never_panic_on_damaged_files_and_bytes_made_by_chance() {
    let bases = [
        ("z_probe.csv", example("z_probe.csv")),
        ("altium_pnp.csv", example("altium_pnp.csv")),
        ("bom_b.csv", example("bom_b.csv")),
        ("panel_marks.json", example("panel_marks.json")),
        ("board_marks.pos", example("board_marks.pos")),
    ];
    for (name, base) in &bases {
        for seed in 0..150u64 {
            let mut rng = Rng::new(seed * 77 + name.len() as u64);
            let mut chars: Vec<char> = base.chars().collect();
            for _ in 0..1 + below(&mut rng, 5) {
                if chars.is_empty() {
                    break;
                }
                let at = below(&mut rng, chars.len());
                match below(&mut rng, 4) {
                    0 => {
                        let end = (at + 1 + below(&mut rng, 10)).min(chars.len());
                        chars.drain(at..end);
                    }
                    1 => chars.insert(
                        at,
                        ['"', ',', ';', '\n', '\u{0}', '9', 'é', '-'][below(&mut rng, 8)],
                    ),
                    2 => chars.truncate(at),
                    _ => chars.insert(at, '9'),
                }
            }
            let text: String = chars.into_iter().collect();
            let result = std::panic::catch_unwind(|| {
                let _ = nozzlez::parse_readings(&text);
                let _ = asmimport::parse(&text);
                let _ = Bom::parse(&text, 1000);
                if let Ok(panel) = Panel::parse(&text) {
                    let _ =
                        panel.expand(&|_| Ok(vec![mark("FID1", 0.0, 0.0), part("R1", 1.0, 1.0)]));
                }
            });
            assert!(result.is_ok(), "{name} seed {seed}: {text:?}");
        }
    }
    for seed in 0..200u64 {
        let mut rng = Rng::new(seed);
        let len = below(&mut rng, 200);
        let bytes: Vec<u8> = (0..len).map(|_| rng.next_u64() as u8).collect();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let result = std::panic::catch_unwind(|| {
            let _ = nozzlez::parse_readings(&text);
            let _ = asmimport::parse(&text);
        });
        assert!(result.is_ok(), "seed {seed}");
    }
}

#[test]
fn the_new_commands_are_given_nothing_a_wrong_option_and_missing_files_and_never_panic() {
    for command in [
        "nozzle-zcal",
        "panel-fids",
        "tape-steps",
        "shift-oee",
        "import-assembler",
        "bom-merge",
    ] {
        for args in [
            vec![command],
            vec![command, "--no-such-option"],
            vec![command, "no_such_a.csv", "no_such_b.csv"],
        ] {
            let o = run(&args);
            assert_eq!(o.status.code(), Some(1), "{args:?}");
            assert!(err(&o).starts_with("error: "), "{args:?}: {}", err(&o));
            assert!(!err(&o).contains("panicked"), "{args:?}");
        }
    }
}
