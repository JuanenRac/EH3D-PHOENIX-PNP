//! The values of the forms of the tools are settings of the machine file: their ranges, their defaults, the commands that take them when an option is not given, and keeping them in the file.

use eh3d_phoenix_pnp::config::{MachineConfig, KEYS, MAX_CONFIG_BYTES};
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::toolvalues::{keep, merge, set_lines, ToolValues};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const MACHINE: &str = include_str!("../examples/machine.cfg");

const NEW_KEYS: [&str; 7] = [
    "parts_diff_tol_mm",
    "parts_diff_rot_tol_deg",
    "board_stats_near_mm",
    "board_stats_limit",
    "feeder_slots_max_plans",
    "simulate_runs_count",
    "journal_steps",
];

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "phoenix_pnp_toolvalues_{tag}_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
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

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn values(text: &str) -> ToolValues {
    ToolValues::from_config(&MachineConfig::parse(text).unwrap())
}

// ---- the settings and their ranges

#[test]
fn the_values_of_the_forms_are_settings_with_the_defaults_the_forms_had() {
    let d = MachineConfig::default();
    assert_eq!((d.parts_diff_tol_mm, d.parts_diff_rot_tol_deg), (0.01, 0.5));
    assert_eq!((d.board_stats_near_mm, d.board_stats_limit), (0.0, 20));
    assert_eq!(d.feeder_slots_max_plans, 5000);
    assert_eq!(d.simulate_runs_count, 20);
    assert_eq!(d.journal_steps, 0);
    for key in NEW_KEYS {
        assert!(KEYS.contains(&key), "{key}");
        assert!(MACHINE.contains(&format!("\n{key} = ")), "{key}");
    }
    // the example file gives what the program has
    assert_eq!(values(MACHINE), ToolValues::from_config(&d));
    // the names are the ones of the lines, in the order of the list of keys
    let lines = ToolValues::from_config(&d).lines();
    let names: Vec<&str> = lines.iter().map(|(k, _)| *k).collect();
    assert_eq!(names, NEW_KEYS);
    let at = KEYS.iter().position(|k| *k == NEW_KEYS[0]).unwrap();
    assert_eq!(&KEYS[at..at + 7], &NEW_KEYS[..]);
}

#[test]
fn each_setting_has_its_range_and_a_value_outside_it_is_refused_with_its_code() {
    for (key, ok, bad) in [
        ("parts_diff_tol_mm", ["0", "100"], ["-0.01", "100.5"]),
        ("parts_diff_rot_tol_deg", ["0", "180"], ["-1", "180.5"]),
        ("board_stats_near_mm", ["0.01", "50"], ["0.005", "50.5"]),
        ("board_stats_limit", ["1", "1000"], ["0", "1001"]),
        ("feeder_slots_max_plans", ["1", "1000000"], ["0", "1000001"]),
        ("simulate_runs_count", ["1", "10000"], ["0", "10001"]),
        ("journal_steps", ["0", "1000000"], ["-1", "1000001"]),
    ] {
        for v in ok {
            assert!(
                MachineConfig::parse(&format!("{key} = {v}")).is_ok(),
                "{key} {v}"
            );
        }
        for v in bad {
            let e = MachineConfig::parse(&format!("{key} = {v}")).unwrap_err();
            assert_eq!(e.code, Code::ConfigOutOfRange, "{key} {v}");
        }
        for v in ["many", "", "1,5", "nan"] {
            assert!(
                MachineConfig::parse(&format!("{key} = {v}")).is_err(),
                "{key} {v:?}"
            );
        }
    }
    // 0 leaves the look for near parts off; a distance under the least that can be looked for is not 0
    assert!(MachineConfig::parse("board_stats_near_mm = 0").is_ok());
    assert!(MachineConfig::parse("board_stats_near_mm = 0.0").is_ok());
    assert!(MachineConfig::parse("board_stats_near_mm = 0.009").is_err());
    // a whole number is a whole number
    assert!(MachineConfig::parse("board_stats_limit = 2.5").is_err());
    // written twice is refused as every key is
    assert_eq!(
        MachineConfig::parse("journal_steps = 1\njournal_steps = 2")
            .unwrap_err()
            .code,
        Code::ConfigDuplicateKey
    );
}

// ---- writing a value into the text of the file

#[test]
fn a_value_replaces_its_line_where_it_stands_and_nothing_else_changes() {
    let text = "# the machine\nnozzles = 2\n  journal_steps =   5\n# journal_steps = 9\njournal_steps_extra = 3\nlanguage = es\n";
    let got = set_lines(text, &[("journal_steps", "40".to_string())]);
    assert_eq!(
        got,
        "# the machine\nnozzles = 2\njournal_steps = 40\n# journal_steps = 9\njournal_steps_extra = 3\nlanguage = es\n"
    );
    // the same text again gives the same text
    assert_eq!(set_lines(&got, &[("journal_steps", "40".to_string())]), got);
}

#[test]
fn a_value_that_has_no_line_is_added_at_the_end_and_the_line_endings_are_kept() {
    let lf = set_lines("nozzles = 2\n", &[("board_stats_limit", "7".to_string())]);
    assert_eq!(lf, "nozzles = 2\nboard_stats_limit = 7\n");
    let crlf = set_lines(
        "nozzles = 2\r\nlanguage = de\r\n",
        &[
            ("board_stats_limit", "7".to_string()),
            ("language", "fr".to_string()),
        ],
    );
    assert_eq!(
        crlf,
        "nozzles = 2\r\nlanguage = fr\r\nboard_stats_limit = 7\r\n"
    );
    // no line end at the end of the text is not a reason to run two lines together
    assert_eq!(
        set_lines("nozzles = 2", &[("journal_steps", "1".to_string())]),
        "nozzles = 2\njournal_steps = 1\n"
    );
    assert_eq!(
        set_lines("", &[("journal_steps", "1".to_string())]),
        "journal_steps = 1\n"
    );
}

#[test]
fn the_numbers_are_written_the_way_the_file_writes_them() {
    let mut v = ToolValues::from_config(&MachineConfig::default());
    v.parts_diff_tol_mm = 0.1 + 0.2;
    v.parts_diff_rot_tol_deg = 12.0;
    v.board_stats_near_mm = 0.01;
    let lines = v.lines();
    let get = |k: &str| lines.iter().find(|(n, _)| *n == k).unwrap().1.clone();
    assert_eq!(get("parts_diff_tol_mm"), "0.3");
    assert_eq!(get("parts_diff_rot_tol_deg"), "12");
    assert_eq!(get("board_stats_near_mm"), "0.01");
    assert_eq!(get("board_stats_limit"), "20");
    v.board_stats_near_mm = 0.0;
    assert_eq!(
        v.lines()
            .iter()
            .find(|(n, _)| *n == "board_stats_near_mm")
            .unwrap()
            .1,
        "0"
    );
    // every number that is written is one the file reads back
    for tol in [0.0, 0.01, 0.123456, 99.999999, 100.0] {
        let mut w = ToolValues::from_config(&MachineConfig::default());
        w.parts_diff_tol_mm = tol;
        let merged = merge("", &w).unwrap();
        let back = values(&merged);
        assert!(
            (back.parts_diff_tol_mm - tol).abs() < 1e-6,
            "{tol} {merged}"
        );
    }
}

#[test]
fn merging_gives_a_file_that_reads_back_the_values_and_is_checked_as_a_whole() {
    let mut v = ToolValues::from_config(&MachineConfig::default());
    v.parts_diff_tol_mm = 0.25;
    v.parts_diff_rot_tol_deg = 3.0;
    v.board_stats_near_mm = 1.5;
    v.board_stats_limit = 8;
    v.feeder_slots_max_plans = 300;
    v.simulate_runs_count = 50;
    v.journal_steps = 12;
    let merged = merge(MACHINE, &v).unwrap();
    assert_eq!(values(&merged), v);
    // the rest of the example file is as it was: its other lines and its comments
    let before: Vec<&str> = MACHINE
        .lines()
        .filter(|l| !NEW_KEYS.iter().any(|k| l.starts_with(&format!("{k} = "))))
        .collect();
    let after: Vec<&str> = merged.lines().collect();
    for l in &before {
        assert!(after.contains(l), "{l}");
    }
    assert_eq!(after.len(), MACHINE.lines().count());
    // merging again changes nothing
    assert_eq!(merge(&merged, &v).unwrap(), merged);
    // a file with no line of them gets seven
    let small = merge("nozzles = 3\n", &v).unwrap();
    assert_eq!(small.lines().count(), 8);
    assert_eq!(values(&small), v);
    assert_eq!(MachineConfig::parse(&small).unwrap().nozzles, 3);
}

#[test]
fn a_value_out_of_its_range_or_a_file_that_is_not_valid_is_refused_and_nothing_is_made() {
    let mut v = ToolValues::from_config(&MachineConfig::default());
    v.board_stats_limit = 0;
    assert_eq!(merge("", &v).unwrap_err().code, Code::ConfigOutOfRange);
    v.board_stats_limit = 5;
    v.parts_diff_tol_mm = f64::NAN;
    assert!(merge("", &v).is_err());
    v.parts_diff_tol_mm = f64::INFINITY;
    assert!(merge("", &v).is_err());
    v.parts_diff_tol_mm = 0.01;
    // a line that is not a setting of the file is not hidden by the merge
    assert!(merge("nozzles = 99\n", &v).is_err());
    assert!(merge("not a setting at all\n", &v).is_err());
    let big = "# x\n".repeat(MAX_CONFIG_BYTES / 4 + 1);
    assert_eq!(merge(&big, &v).unwrap_err().code, Code::FileTooBig);
}

// ---- keeping them in the file

#[test]
fn keeping_the_values_changes_the_file_and_leaves_no_other_file() {
    let dir = scratch("keep");
    let path = dir.join("machine.cfg");
    std::fs::write(&path, "# mine\r\nnozzles = 2\r\njournal_steps = 3\r\n").unwrap();
    let mut v = values("nozzles = 2\n");
    v.journal_steps = 99;
    v.simulate_runs_count = 77;
    keep(&path, &v).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.starts_with("# mine\r\nnozzles = 2\r\njournal_steps = 99\r\n"),
        "{text:?}"
    );
    assert!(text.contains("simulate_runs_count = 77\r\n"), "{text:?}");
    assert!(!text.contains("\n\n") && !text.replace("\r\n", "").contains('\n'));
    assert_eq!(values(&text), v);
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["machine.cfg"], "{names:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn keeping_never_creates_a_file_and_never_changes_one_that_is_not_valid_or_a_value_out_of_range() {
    let dir = scratch("keep_bad");
    // a file that is not there is not made
    let missing = dir.join("none.cfg");
    let v = values("");
    assert_eq!(keep(&missing, &v).unwrap_err().code, Code::DriverError);
    assert!(!missing.exists());
    // a file that is not a configuration is left as it is
    let broken = dir.join("broken.cfg");
    std::fs::write(&broken, "this is not a setting\n").unwrap();
    assert!(keep(&broken, &v).is_err());
    assert_eq!(
        std::fs::read_to_string(&broken).unwrap(),
        "this is not a setting\n"
    );
    // a value out of its range is left out of the file
    let fine = dir.join("fine.cfg");
    std::fs::write(&fine, "nozzles = 2\n").unwrap();
    let mut wrong = values("");
    wrong.simulate_runs_count = 0;
    assert_eq!(
        keep(&fine, &wrong).unwrap_err().code,
        Code::ConfigOutOfRange
    );
    assert_eq!(std::fs::read_to_string(&fine).unwrap(), "nozzles = 2\n");
    // bytes that are not text
    let bytes = dir.join("bytes.cfg");
    std::fs::write(&bytes, [0xff, 0xfe, 0x00, 0x41]).unwrap();
    assert!(keep(&bytes, &v).is_err());
    // a file with a byte order mark is read as the text it is
    let bom = dir.join("bom.cfg");
    std::fs::write(&bom, "\u{feff}nozzles = 2\n").unwrap();
    keep(&bom, &v).unwrap();
    assert!(std::fs::read_to_string(&bom)
        .unwrap()
        .starts_with("nozzles = 2\n"));
    // a folder is not a file
    assert!(keep(&dir, &v).is_err());
    let names = std::fs::read_dir(&dir).unwrap().count();
    assert_eq!(names, 4, "no .tmp is left behind");
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the commands take them when the option is not given

fn table(rows: &[(&str, f64, f64)]) -> String {
    let mut t = String::from("ref,val,package,x,y,rot,side\n");
    for (r, x, y) in rows {
        t.push_str(&format!("{r},10k,R_0603,{x},{y},0,top\n"));
    }
    t
}

#[test]
fn parts_diff_takes_its_tolerances_from_the_file_and_an_option_wins() {
    let dir = scratch("diff");
    let (a, b) = (dir.join("a.csv"), dir.join("b.csv"));
    std::fs::write(&a, table(&[("R1", 10.0, 10.0), ("R2", 20.0, 10.0)])).unwrap();
    std::fs::write(&b, table(&[("R1", 10.05, 10.0), ("R2", 20.0, 10.0)])).unwrap();
    let loose = dir.join("loose.cfg");
    std::fs::write(&loose, "parts_diff_tol_mm = 0.1\n").unwrap();
    // the default tolerance (0.01 mm) sees the move
    let o = run(&["parts-diff", p(&a), p(&b)]);
    assert!(!o.status.success(), "{}", out(&o));
    assert!(out(&o).contains("R1"), "{}", out(&o));
    // the file says 0.1 mm: the same tables are the same
    let o = run(&["parts-diff", p(&a), p(&b), "--config", p(&loose)]);
    assert!(
        o.status.success(),
        "{}{}",
        out(&o),
        String::from_utf8_lossy(&o.stderr)
    );
    // the option is the stronger
    let o = run(&[
        "parts-diff",
        p(&a),
        p(&b),
        "--config",
        p(&loose),
        "--tol",
        "0.01",
    ]);
    assert!(!o.status.success());
    // the turn too
    std::fs::write(
        &b,
        "ref,val,package,x,y,rot,side\nR1,10k,R_0603,10,10,3,top\nR2,10k,R_0603,20,10,0,top\n",
    )
    .unwrap();
    let wide = dir.join("wide.cfg");
    std::fs::write(&wide, "parts_diff_rot_tol_deg = 5\n").unwrap();
    assert!(!run(&["parts-diff", p(&a), p(&b)]).status.success());
    assert!(run(&["parts-diff", p(&a), p(&b), "--config", p(&wide)])
        .status
        .success());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn board_stats_takes_the_distance_and_the_pairs_from_the_file() {
    let dir = scratch("stats");
    let cfg = dir.join("near.cfg");
    std::fs::write(&cfg, "board_stats_near_mm = 12\nboard_stats_limit = 2\n").unwrap();
    // without it the look for near parts is off
    let o = run(&["board-stats", "examples/board.pos"]);
    assert!(o.status.success());
    assert!(!out(&o).contains("closer than"), "{}", out(&o));
    // with it: the pairs are looked for and only 2 are listed
    let o = run(&["board-stats", "examples/board.pos", "--config", p(&cfg)]);
    let text = out(&o);
    assert!(!o.status.success(), "{text}");
    assert!(
        text.contains("9 pairs of parts closer than 12 mm"),
        "{text}"
    );
    assert!(text.contains("... and 7 more"), "{text}");
    assert_eq!(text.matches(" mm apart").count(), 2, "{text}");
    // the options win
    let o = run(&[
        "board-stats",
        "examples/board.pos",
        "--config",
        p(&cfg),
        "--near",
        "10.5",
        "--limit",
        "20",
    ]);
    let text = out(&o);
    assert!(
        text.contains("pairs of parts closer than 10.5 mm"),
        "{text}"
    );
    assert!(!text.contains("more\n"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn feeder_slots_and_the_runs_take_their_counts_from_the_file() {
    let dir = scratch("counts");
    let cfg = dir.join("few.cfg");
    std::fs::write(
        &cfg,
        "feeder_slots_max_plans = 1\nsimulate_runs_count = 3\n",
    )
    .unwrap();
    let base = ["examples/board.pos", "examples/feeders.csv"];
    let go = |cmd: &str, extra: &[&str]| -> String {
        let mut v = vec![cmd];
        v.extend_from_slice(&base);
        v.extend_from_slice(extra);
        out(&run(&v))
    };
    assert!(
        go("feeder-slots", &["--config", p(&cfg)]).contains("(1 plans tried, the search was cut)")
    );
    assert!(go(
        "feeder-slots",
        &["--config", p(&cfg), "--max-plans", "5000"]
    )
    .contains("plans tried"));
    assert!(!go(
        "feeder-slots",
        &["--config", p(&cfg), "--max-plans", "5000"]
    )
    .contains("the search was cut"));
    assert!(go("simulate-runs", &["--config", p(&cfg)]).starts_with("3 runs from the seed"));
    assert!(go("simulate-runs", &["--config", p(&cfg), "--runs", "2"])
        .starts_with("2 runs from the seed"));
    assert!(go("simulate-runs", &[]).starts_with("20 runs from the seed"));
    assert!(
        go("simulate-hotspots", &["--config", p(&cfg)]).contains("in 3 runs from the seed")
            || go("simulate-hotspots", &["--config", p(&cfg)])
                .contains("no feeder gave trouble in 3 runs")
    );
    // a file with a value out of range is refused by every command
    let bad = dir.join("bad.cfg");
    std::fs::write(&bad, "simulate_runs_count = 0\n").unwrap();
    assert!(
        !run(&["simulate-runs", base[0], base[1], "--config", p(&bad)])
            .status
            .success()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
