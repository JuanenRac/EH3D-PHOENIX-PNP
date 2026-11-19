//! The board planned with one nozzle, then two, up to six: the rows, the fastest, what each nozzle takes off the one before, the heads of the configuration left out, the text, the table and
//! `nozzle-compare`.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::nozzles::{compare, csv, fastest, gain_s, text, Figures, Row, MAX_NOZZLES};
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::Options;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nR3,10k,R,30,10,180,top\nC1,100n,C,10,20,0,top\nC2,100n,C,20,20,0,top\nU1,MCU,Q,40,15,0,top\n";
const FEEDERS: &str =
    "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\n";

fn rows(cfg: &str, most: usize) -> Vec<Row> {
    let cfg = MachineConfig::parse(cfg).unwrap();
    let parts = from_csv(PARTS).unwrap();
    let feeders = feeding::from_csv(FEEDERS).unwrap();
    let picks = assign(&parts, &feeders, Some(Side::Top));
    compare(
        &parts,
        &feeders,
        &picks,
        &Catalog::new(),
        &cfg,
        &Options::default(),
        most,
    )
}

fn row(nozzles: usize, seconds: f64) -> Row {
    Row {
        nozzles,
        plan: Ok(Figures {
            batches: 6usize.div_ceil(nozzles),
            tip_changes: 0,
            travel_mm: 100.0 / nozzles as f64,
            seconds,
        }),
    }
}

#[test]
fn the_board_is_planned_with_each_number_of_nozzles_up_to_the_one_asked() {
    let all = rows("", 6);
    assert_eq!(all.len(), 6);
    assert_eq!(
        all.iter().map(|r| r.nozzles).collect::<Vec<_>>(),
        [1, 2, 3, 4, 5, 6]
    );
    // the parts are picked in batches of as many as the nozzles
    let batches: Vec<usize> = all
        .iter()
        .map(|r| r.plan.as_ref().unwrap().batches)
        .collect();
    assert_eq!(batches, [6, 3, 2, 2, 2, 1]);
    // fewer rows when fewer are asked for, and the number is held to 1 to 6
    assert_eq!(rows("", 3).len(), 3);
    assert_eq!(rows("", 0).len(), 1);
    assert_eq!(rows("", 99).len(), MAX_NOZZLES);
    // the same board and settings give the same rows
    assert_eq!(rows("", 6), all);
}

#[test]
fn the_nozzles_of_the_configuration_do_not_change_what_is_compared_and_its_heads_are_left_out() {
    let plain = rows("", 6);
    assert_eq!(rows("nozzles = 4\n", 6), plain);
    assert_eq!(rows("head = a : 2\nhead = b : 3\n", 6), plain);
}

#[test]
fn the_fastest_has_the_fewest_seconds_then_the_fewest_nozzles_and_a_failed_plan_is_skipped() {
    let slow_first = [row(1, 10.0), row(2, 8.0), row(3, 8.0), row(4, 9.0)];
    assert_eq!(fastest(&slow_first), Some(1));
    let failed = Row {
        nozzles: 2,
        plan: Err("a setting does not allow it".to_string()),
    };
    assert_eq!(
        fastest(&[row(1, 9.0), failed.clone(), row(3, 7.0)]),
        Some(2)
    );
    assert_eq!(fastest(std::slice::from_ref(&failed)), None);
    assert_eq!(fastest(&[]), None);
}

#[test]
fn the_gain_of_a_nozzle_is_what_it_takes_off_the_one_before() {
    let list = [
        row(1, 10.0),
        row(2, 8.5),
        Row {
            nozzles: 3,
            plan: Err("no".to_string()),
        },
        row(4, 8.0),
    ];
    assert_eq!(gain_s(&list, 0), None);
    assert_eq!(gain_s(&list, 1), Some(1.5));
    // a plan that could not be made has no gain and gives none to the next
    assert_eq!(gain_s(&list, 2), None);
    assert_eq!(gain_s(&list, 3), None);
    assert_eq!(gain_s(&list, 9), None);
}

#[test]
fn the_text_marks_the_configuration_and_the_fastest_and_says_what_it_would_save() {
    let list = [row(1, 10.0), row(2, 8.0), row(3, 7.0)];
    let t = text(&list, 1, 0);
    let lines: Vec<&str> = t.lines().collect();
    assert_eq!(
        lines[0],
        "  nozzles batches  tips  travel mm   seconds    gain s"
    );
    assert_eq!(
        lines[1],
        "  1             6     0      100.0      10.0         -  <- the configuration"
    );
    assert!(lines[3].ends_with("<- fastest"), "{t}");
    assert!(lines[2].ends_with("2.0") || lines[2].contains("2.0"), "{t}");
    assert_eq!(
        lines[4],
        "3 nozzles would take 3.0 s off the 10.0 s of the plan with the 1 nozzles of the configuration"
    );
    // when the configuration already has the fastest, the line says only which it is
    let t = text(&list, 3, 0);
    assert!(
        t.ends_with("the fastest plan is the one with 3 nozzles\n"),
        "{t}"
    );
    // the heads that were set aside are told first
    let t = text(&list, 1, 2);
    assert!(
        t.starts_with("the 2 heads of the configuration are left out"),
        "{t}"
    );
}

#[test]
fn a_plan_that_cannot_be_made_is_told_in_the_text_and_in_the_table_and_none_at_all_is_said() {
    let list = [
        row(1, 10.0),
        Row {
            nozzles: 2,
            plan: Err("a, b \"c\"".to_string()),
        },
    ];
    let t = text(&list, 1, 0);
    assert!(t.contains("  2       no plan: a, b \"c\"\n"), "{t}");
    let c = csv(&list);
    assert_eq!(
        c,
        "nozzles,batches,tip_changes,travel_mm,seconds,gain_s,error\n1,6,0,100.000,10.000,,\n2,,,,,,\"a, b \"\"c\"\"\"\n"
    );
    let none = [Row {
        nozzles: 1,
        plan: Err("no".to_string()),
    }];
    assert!(text(&none, 1, 0).ends_with("no plan could be made with any number of nozzles\n"));
    assert!(text(&[], 1, 0).ends_with("no plan could be made with any number of nozzles\n"));
}

// ---- nozzle-compare

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

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn the_command_plans_the_board_with_each_number_of_nozzles() {
    let o = run(&[
        "nozzle-compare",
        "examples/board.pos",
        "examples/feeders.csv",
    ]);
    let t = out(&o);
    assert!(
        o.status.success(),
        "{t}{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let lines: Vec<&str> = t.lines().collect();
    assert!(
        lines[0].contains("nozzles") && lines[0].contains("gain s"),
        "{t}"
    );
    assert_eq!(
        lines.iter().filter(|l| l.contains("<- fastest")).count(),
        1,
        "{t}"
    );
    assert_eq!(
        lines
            .iter()
            .filter(|l| l.contains("<- the configuration"))
            .count(),
        1,
        "{t}"
    );
    assert_eq!(
        lines.iter().filter(|l| l.starts_with("  ")).count(),
        7,
        "{t}"
    );
    // --max holds the rows
    let o = run(&[
        "nozzle-compare",
        "examples/board.pos",
        "examples/feeders.csv",
        "--max",
        "3",
    ]);
    assert_eq!(out(&o).lines().filter(|l| l.starts_with("  ")).count(), 4);
    // the configuration with two heads says they were left out
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_nozzles_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("heads.cfg");
    std::fs::write(&cfg, "head = a : 2\nhead = b : 3\n").unwrap();
    let o = run(&[
        "nozzle-compare",
        "examples/board.pos",
        "examples/feeders.csv",
        "--config",
        p(&cfg),
    ]);
    assert!(
        out(&o).starts_with("the 2 heads of the configuration are left out"),
        "{}",
        out(&o)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_command_writes_the_table_never_over_a_file_and_refuses_bad_words() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_nozzles2_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("nozzles.csv");
    let base = [
        "nozzle-compare",
        "examples/board.pos",
        "examples/feeders.csv",
    ];
    let mut args = base.to_vec();
    args.extend(["--out", p(&file)]);
    let o = run(&args);
    assert!(o.status.success(), "{}", out(&o));
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.starts_with("nozzles,batches,tip_changes,travel_mm,seconds,gain_s,error\n1,6,0,"),
        "{text}"
    );
    assert_eq!(text.lines().count(), 7);
    assert!(!run(&args).status.success());
    let mut over = base.to_vec();
    over.extend(["--out", "examples/board.pos"]);
    assert!(!run(&over).status.success());
    for bad in ["0", "7", "2.5", "x", "-1"] {
        let mut a = base.to_vec();
        a.extend(["--max", bad]);
        assert!(!run(&a).status.success(), "{bad}");
    }
    assert!(!run(&["nozzle-compare", "examples/board.pos"])
        .status
        .success());
    assert!(out(&run(&["help"])).contains("phoenix-pnp nozzle-compare PARTS FEEDERS [--max N]"));
    let _ = std::fs::remove_dir_all(&dir);
}
