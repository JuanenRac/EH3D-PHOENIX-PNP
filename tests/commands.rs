//! The commands that use the configuration of the machine, run as a person would run them on the files of examples/.

use std::path::PathBuf;
use std::process::{Command, Output};

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
    let dir = std::env::temp_dir().join(format!("eh3d_pnp_cmd_{}_{name}", std::process::id()));
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

fn path(dir: &std::path::Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

#[test]
fn schedule_writes_the_plan_and_the_reports_of_the_example() {
    let dir = scratch("schedule");
    let (plan, report, gcode, missing) = (
        path(&dir, "plan.json"),
        path(&dir, "report.txt"),
        path(&dir, "moves.gcode"),
        path(&dir, "missing.csv"),
    );
    let o = run(&[
        "schedule",
        &example("board.pos"),
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
        "--gcode",
        &gcode,
        "--missing",
        &missing,
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&plan).unwrap()).unwrap();
    assert_eq!(v["schema_version"], 2);
    assert_eq!(
        v["placements"].as_array().unwrap().len(),
        6,
        "the six parts of the top side"
    );
    // the microcontroller is of the second block of ranks and goes last
    assert_eq!(v["placements"][5]["reference"], "U1");
    assert_eq!(v["tip_changes"], 1);
    assert!(std::fs::read_to_string(&report)
        .unwrap()
        .contains("6 parts in"));
    assert!(std::fs::read_to_string(&gcode).unwrap().contains("G21"));
    assert!(std::fs::read_to_string(&missing)
        .unwrap()
        .starts_with("kind,what,detail"));
    assert!(out(&o).contains("6 parts in"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn schedule_plans_a_whole_panel() {
    let dir = scratch("panel");
    let plan = path(&dir, "plan.json");
    let o = run(&[
        "schedule",
        "--panel",
        &example("panel.json"),
        &example("feeders.csv"),
        &plan,
        "--config",
        &example("machine.cfg"),
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
    assert_eq!(refs.len(), 12);
    assert!(refs
        .iter()
        .all(|r| r.starts_with("A/") || r.starts_with("B/")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn preflight_prints_what_it_finds_and_fails_on_an_error() {
    let ok = run(&[
        "preflight",
        &example("board.pos"),
        &example("feeders.csv"),
        "--config",
        &example("machine.cfg"),
        "--packages",
        &example("packages.csv"),
    ]);
    assert!(ok.status.success(), "{}", err(&ok));
    assert!(out(&ok).contains("info: setting rank_step"), "{}", out(&ok));
    let dir = scratch("preflight");
    let cfg = path(&dir, "tight.cfg");
    std::fs::write(&cfg, "limit_x_max_mm = 15\n").unwrap();
    let bad = run(&[
        "preflight",
        &example("board.pos"),
        &example("feeders.csv"),
        "--config",
        &cfg,
    ]);
    assert_eq!(bad.status.code(), Some(1));
    assert!(out(&bad).contains("error: part "), "{}", out(&bad));
    assert!(err(&bad).contains("PHOENIX-PNP-028"), "{}", err(&bad));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn simulate_pauses_saves_its_state_and_resumes() {
    let dir = scratch("simulate");
    let (state, report) = (path(&dir, "state.json"), path(&dir, "report.json"));
    let base = [
        "simulate",
        &example("board.pos"),
        &example("feeders.csv"),
        "--config",
        &example("machine.cfg"),
        "--packages",
        &example("packages.csv"),
        "--state",
        &state,
    ];
    let mut first: Vec<&str> = base.to_vec();
    first.extend(["--steps", "5"]);
    let a = run(&first);
    assert!(a.status.success(), "{}", err(&a));
    assert!(out(&a).contains("paused"), "{}", out(&a));
    assert!(std::path::Path::new(&state).exists());
    let mut second: Vec<&str> = base.to_vec();
    second.extend(["--report", &report]);
    let b = run(&second);
    assert!(b.status.success(), "{}", err(&b));
    let text = out(&b);
    assert!(text.contains("resumed from"), "{text}");
    assert!(text.contains("placed 6 of 6"), "{text}");
    assert!(text.contains("finished"), "{text}");
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report).unwrap()).unwrap();
    assert_eq!(v["placed"], 6);
    // the same state with another board is refused
    let other = run(&[
        "simulate",
        &example("board.pos"),
        &example("feeders.csv"),
        "--nozzles",
        "1",
        "--state",
        &state,
    ]);
    assert_eq!(other.status.code(), Some(1));
    assert!(err(&other).contains("PHOENIX-PNP-025"), "{}", err(&other));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn simulate_shows_a_part_that_is_put_aside() {
    let dir = scratch("defer");
    let cfg = path(&dir, "failing.cfg");
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
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    assert!(text.contains("placed 0 of 6, 6 put aside"), "{text}");
    assert!(text.contains("put aside: "), "{text}");
    // with alert the job stops with an error
    std::fs::write(&cfg, "sim_pick_fail_rate = 1\n").unwrap();
    let stop = run(&[
        "simulate",
        &example("board.pos"),
        &example("feeders.csv"),
        "--config",
        &cfg,
    ]);
    assert_eq!(stop.status.code(), Some(1));
    assert!(err(&stop).contains("PHOENIX-PNP-022"), "{}", err(&stop));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn loadsheet_gives_each_kind_a_slot_and_writes_the_feeders() {
    let dir = scratch("loadsheet");
    let (sheet, feeders) = (path(&dir, "sheet.csv"), path(&dir, "feeders.csv"));
    let o = run(&[
        "loadsheet",
        &example("board.pos"),
        &example("slots.csv"),
        &sheet,
        "--feeders-out",
        &feeders,
        "--packages",
        &example("packages.csv"),
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(
        out(&o).contains("3 kinds of part in slots, 0 without a slot"),
        "{}",
        out(&o)
    );
    let csv = std::fs::read_to_string(&sheet).unwrap();
    assert!(csv.starts_with("slot,value,package,count,tape_mm,tip,x,y\n"));
    // the wide tape of the microcontroller is in a wide slot
    assert!(
        csv.lines()
            .any(|l| l.contains(",MCU,") && (l.starts_with("S3,") || l.starts_with("S4,"))),
        "{csv}"
    );
    // the feeders it writes are good for the planner
    let plan = path(&dir, "plan.json");
    let p = run(&["plan", &example("board.pos"), &feeders, &plan]);
    assert!(p.status.success(), "{}", err(&p));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn loadsheet_fails_when_a_kind_has_no_slot_wide_enough() {
    let dir = scratch("narrow");
    let slots = path(&dir, "slots.csv");
    std::fs::write(&slots, "id,x,y,max_width_mm\nA,0,0,8\nB,5,5,8\nC,9,9,8\n").unwrap();
    let sheet = path(&dir, "sheet.csv");
    let o = run(&[
        "loadsheet",
        &example("board.pos"),
        &slots,
        &sheet,
        "--packages",
        &example("packages.csv"),
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(out(&o).contains("no slot for MCU / QFN-32"), "{}", out(&o));
    assert!(
        std::path::Path::new(&sheet).exists(),
        "the sheet is written anyway"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn config_check_says_ok_or_names_the_code_in_the_language_asked() {
    let ok = run(&["config-check", &example("machine.cfg")]);
    assert!(ok.status.success(), "{}", err(&ok));
    assert!(
        out(&ok).starts_with("ok: schema 1, 2 nozzles, order nearest, 2 rotation rules"),
        "{}",
        out(&ok)
    );
    let dir = scratch("config");
    let bad = path(&dir, "bad.cfg");
    std::fs::write(&bad, "colour = red\n").unwrap();
    let en = run(&["config-check", &bad]);
    assert_eq!(en.status.code(), Some(1));
    assert!(
        err(&en).contains("PHOENIX-PNP-004: The configuration has a key that is not known"),
        "{}",
        err(&en)
    );
    assert!(err(&en).contains("line 1"));
    let es = run(&["config-check", &bad, "--lang", "es"]);
    assert!(
        err(&es).contains("La configuración tiene una clave que no se conoce"),
        "{}",
        err(&es)
    );
    let pt = run(&["config-check", &bad, "--lang", "pt"]);
    assert!(
        err(&pt).contains("A configuração tem uma chave desconhecida"),
        "{}",
        err(&pt)
    );
    let xx = run(&["config-check", &bad, "--lang", "xx"]);
    assert!(err(&xx).contains("--lang 'xx'"), "{}", err(&xx));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_larger_than_the_limit_of_the_configuration_is_refused() {
    let dir = scratch("big");
    let cfg = path(&dir, "small.cfg");
    std::fs::write(&cfg, "max_file_bytes = 1024\n").unwrap();
    let big = path(&dir, "big.csv");
    let mut text = String::from("ref,val,package,x,y,rot,side\n");
    for i in 0..200 {
        text.push_str(&format!("R{i},10k,R_0603,{i},0,0,top\n"));
    }
    std::fs::write(&big, text).unwrap();
    let o = run(&["preflight", &big, &example("feeders.csv"), "--config", &cfg]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("PHOENIX-PNP-001"), "{}", err(&o));
    // the old commands have a limit too
    let old = run(&["parts", &big]);
    assert!(old.status.success(), "{}", err(&old));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_new_commands_refuse_to_write_over_their_inputs_and_unknown_options() {
    let feeders = example("feeders.csv");
    let o = run(&["schedule", &example("board.pos"), &feeders, &feeders]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        err(&o).contains("the output file is also an input"),
        "{}",
        err(&o)
    );
    let w = run(&[
        "schedule",
        &example("board.pos"),
        &feeders,
        "out.json",
        "--oops",
        "1",
    ]);
    assert_eq!(w.status.code(), Some(1));
    assert!(err(&w).contains("--oops"), "{}", err(&w));
    let extra = run(&["preflight", &example("board.pos"), &feeders, "extra"]);
    assert_eq!(extra.status.code(), Some(1));
    assert!(
        err(&extra).contains("unexpected argument"),
        "{}",
        err(&extra)
    );
    let missing = run(&["simulate", &example("board.pos")]);
    assert_eq!(missing.status.code(), Some(1));
    for bad in [
        ["preflight", "a", "b", "--side", "left"],
        ["preflight", "a", "b", "--nozzles", "9"],
        ["preflight", "a", "b", "--order", "random"],
        ["preflight", "a", "b", "--speed", "0"],
    ] {
        let o = run(&bad);
        assert_eq!(o.status.code(), Some(1), "{bad:?}");
    }
    assert!(!root().join("out.json").exists());
}

#[test]
fn the_options_of_the_command_line_win_over_the_configuration() {
    let dir = scratch("override");
    let plan = path(&dir, "plan.json");
    let o = run(&[
        "schedule",
        &example("board.pos"),
        &example("feeders.csv"),
        &plan,
        "--config",
        &example("machine.cfg"),
        "--nozzles",
        "1",
        "--order",
        "part",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&plan).unwrap()).unwrap();
    assert_eq!(v["order"], "part");
    assert_eq!(v["batches"], 6, "one nozzle: one part in each batch");
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the fiducials of a panel and the angles of the bottom side

fn placement<'a>(v: &'a serde_json::Value, reference: &str) -> &'a serde_json::Value {
    v["placements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["reference"] == reference)
        .unwrap_or_else(|| panic!("{reference} is not in the plan"))
}

fn plan_of(path: &str) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn the_measured_fiducials_move_the_parts_of_the_panel_in_the_plan() {
    let dir = scratch("fiducials");
    // the panel has f1 at (-5, -5) and f2 at (105, -5): measured one millimetre to the right and two up
    let marks = path(&dir, "marks.csv");
    std::fs::write(&marks, "id,x,y\nf1,-4,-3\nf2,106,-3\n").unwrap();
    let (plain, moved) = (path(&dir, "plain.json"), path(&dir, "moved.json"));
    let (panel, feeders, config) = (
        example("panel.json"),
        example("feeders.csv"),
        example("machine.cfg"),
    );
    let o = run(&[
        "schedule", &feeders, &plain, "--panel", &panel, "--config", &config,
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let o = run(&[
        "schedule",
        &feeders,
        &moved,
        "--panel",
        &panel,
        "--config",
        &config,
        "--fiducials",
        &marks,
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(
        out(&o).contains("fiducials: 2 marks, similarity fit"),
        "{}",
        out(&o)
    );
    let (a, b) = (plan_of(&plain), plan_of(&moved));
    for reference in ["A/R1", "B/R1", "A/U1"] {
        let (p, q) = (
            &placement(&a, reference)["place"],
            &placement(&b, reference)["place"],
        );
        let d = |k: &str| q[k].as_f64().unwrap() - p[k].as_f64().unwrap();
        assert!((d("x") - 1.0).abs() < 1e-9, "{reference}: {}", d("x"));
        assert!((d("y") - 2.0).abs() < 1e-9, "{reference}: {}", d("y"));
        assert!(d("rotation").abs() < 1e-9, "{reference}: {}", d("rotation"));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_fiducial_fit_that_misses_too_much_is_refused_unless_the_configuration_allows_it() {
    let dir = scratch("fiducials_limit");
    std::fs::copy(example("board.pos"), path(&dir, "board.pos")).unwrap();
    let panel = path(&dir, "panel.json");
    std::fs::write(
        &panel,
        r#"{"boards": {"main": "board.pos"},
            "panels": {"p": {"children": [{"id": "A", "board": "main"}],
              "fiducials": [{"id": "f1", "x": 0, "y": 0}, {"id": "f2", "x": 100, "y": 0},
                            {"id": "f3", "x": 100, "y": 100}, {"id": "f4", "x": 0, "y": 100}]}},
            "root": "p"}"#,
    )
    .unwrap();
    // the last mark is four millimetres off: the fit leaves one millimetre on each mark
    let marks = path(&dir, "marks.csv");
    std::fs::write(&marks, "id,x,y\nf1,0,0\nf2,100,0\nf3,100,100\nf4,0,104\n").unwrap();
    let plan = path(&dir, "plan.json");
    let args: [&str; 7] = [
        "schedule",
        &example("feeders.csv"),
        &plan,
        "--panel",
        &panel,
        "--fiducials",
        &marks,
    ];
    let o = run(&args);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("PHOENIX-PNP-032"), "{}", err(&o));
    assert!(err(&o).contains("the limit is 0.5 mm"), "{}", err(&o));
    assert!(!std::path::Path::new(&plan).exists());
    let config = path(&dir, "machine.cfg");
    std::fs::write(&config, "fiducial_max_residual_mm = 1.5\n").unwrap();
    let mut with_config = args.to_vec();
    with_config.extend(["--config", config.as_str()]);
    let o = run(&with_config);
    assert!(o.status.success(), "{}", err(&o));
    assert!(
        out(&o).contains("fiducials: 4 marks, affine fit"),
        "{}",
        out(&o)
    );
    assert!(out(&o).contains("(largest 1.000 mm)"), "{}", out(&o));
    // forcing a similarity on four marks leaves 2 mm on the mark that is off (by hand: the turn is -0.01, the scale 1.01) and nothing on the opposite one
    std::fs::write(
        &config,
        "fiducial_max_residual_mm = 2.5\nfiducial_mode = similarity\n",
    )
    .unwrap();
    let o = run(&with_config);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("4 marks, similarity fit"), "{}", out(&o));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fiducials_need_a_panel_and_a_file_that_is_valid() {
    let dir = scratch("fiducials_bad");
    let marks = path(&dir, "marks.csv");
    std::fs::write(&marks, "id,x,y\nf1,0,0\nf2,1,1\n").unwrap();
    let plan = path(&dir, "plan.json");
    let o = run(&[
        "schedule",
        &example("board.pos"),
        &example("feeders.csv"),
        &plan,
        "--fiducials",
        &marks,
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("--panel"), "{}", err(&o));
    // a mark the panel does not have
    std::fs::write(
        &marks,
        "id,x,y
zz,0,0
f2,1,1
",
    )
    .unwrap();
    let o = run(&[
        "schedule",
        &example("feeders.csv"),
        &plan,
        "--panel",
        &example("panel.json"),
        "--fiducials",
        &marks,
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        err(&o).contains("the mark 'zz' is not in the panel"),
        "{}",
        err(&o)
    );
    assert!(!std::path::Path::new(&plan).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

fn tokens(text: &str, first: &str) -> Vec<String> {
    text.lines()
        .find(|l| l.split_whitespace().next() == Some(first))
        .unwrap_or_else(|| panic!("no line for {first} in {text}"))
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

#[test]
fn check_bottom_shows_the_angle_each_rule_gives() {
    // the example board has one part on the bottom side, D1 at 0 degrees
    let o = run(&["check-bottom", &example("board.pos")]);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    assert!(
        text.starts_with("1 of 7 parts are on the bottom side; the rule in use is keep\n"),
        "{text}"
    );
    assert_eq!(
        tokens(&text, "D1"),
        ["D1", "LED", "LED_0603", "0.00", "0.00", "180.00", "0.00"]
    );
    assert!(tokens(&text, "reference").contains(&"half_turn".to_string()));
    // a file of the own: the angles 30 and 270, a part on the top that is not listed, and the rule mirror in the configuration
    let dir = scratch("check_bottom");
    let positions = path(&dir, "bottom.csv");
    std::fs::write(
        &positions,
        "ref,val,package,x,y,rot,side\nC5,100n,C_0603,10,10,30,bottom\nC6,100n,C_0603,20,10,270,bottom\nR1,10k,R,5,5,45,top\n",
    )
    .unwrap();
    let o = run(&["check-bottom", &positions]);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    assert!(text.starts_with("2 of 3 parts are on the bottom side; the rule in use is keep\n"));
    assert_eq!(
        tokens(&text, "C5"),
        ["C5", "100n", "C_0603", "30.00", "330.00", "210.00", "30.00"]
    );
    assert_eq!(
        tokens(&text, "C6"),
        ["C6", "100n", "C_0603", "270.00", "90.00", "90.00", "270.00"]
    );
    assert!(!text.contains("R1"));
    let config = path(&dir, "machine.cfg");
    std::fs::write(&config, "bottom_rotation = mirror\n").unwrap();
    let o = run(&["check-bottom", &positions, "--config", &config]);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    assert!(text.contains("the rule in use is mirror"));
    // the first columns are still the angles of the file, whatever the rule in use: only the last column follows the rule
    assert_eq!(
        tokens(&text, "C5"),
        ["C5", "100n", "C_0603", "30.00", "330.00", "210.00", "330.00"]
    );
    assert_eq!(
        tokens(&text, "C6"),
        ["C6", "100n", "C_0603", "270.00", "90.00", "90.00", "90.00"]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_bottom_with_no_part_on_the_bottom_and_with_bad_arguments() {
    let dir = scratch("check_bottom_none");
    let top = path(&dir, "top.csv");
    std::fs::write(&top, "ref,val,package,x,y,rot,side\nR1,10k,R,5,5,45,top\n").unwrap();
    let o = run(&["check-bottom", &top]);
    assert!(o.status.success());
    assert_eq!(
        out(&o),
        "0 of 1 parts are on the bottom side; the rule in use is keep\n"
    );
    assert_eq!(run(&["check-bottom"]).status.code(), Some(1));
    assert_eq!(run(&["check-bottom", &top, "extra"]).status.code(), Some(1));
    assert_eq!(
        run(&["check-bottom", &top, "--rule", "mirror"])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        run(&["check-bottom", "/no/such/file.csv"]).status.code(),
        Some(1)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn simulate_lists_the_failures_by_package_and_by_feeder_and_the_parts_per_hour() {
    let dir = scratch("sim_stats");
    let config = path(&dir, "machine.cfg");
    // every pick of the package C_0603 and every pick from the feeder F3 fails, and the parts that fail are put aside
    std::fs::write(
        &config,
        "sim_part_fail = C_0603 : 1 : 0\nsim_feeder_fail = F3 : 1\nerror_policy = defer\npick_retries = 0\nfault_limit = 0\n",
    )
    .unwrap();
    let report = path(&dir, "report.json");
    let o = run(&[
        "simulate",
        &example("board.pos"),
        &example("feeders.csv"),
        "--config",
        &config,
        "--report",
        &report,
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    // the top side has R1 R2 R3 (R_0603), C1 C2 (C_0603) and U1 (QFN-32): only the three resistors are placed
    assert!(text.contains("placed 3 of 6, 3 put aside"), "{text}");
    assert!(text.contains("parts per hour (simulated)"), "{text}");
    assert!(
        text.contains("package C_0603: 0 picked (2 failed), 0 placed (0 failed)"),
        "{text}"
    );
    assert!(
        text.contains("package R_0603: 3 picked (0 failed), 3 placed (0 failed)"),
        "{text}"
    );
    assert!(
        text.contains("feeder F3: 0 picked, 1 failed picks"),
        "{text}"
    );
    let v = plan_of(&report);
    assert_eq!(v["placed"], 3);
    assert!(v["parts_per_hour"].as_f64().unwrap() > 0.0);
    assert_eq!(v["kinds"].as_array().unwrap().len(), 4);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- boards marked as bad and the marks of two levels

fn two_level_files(dir: &std::path::Path) -> (String, String) {
    std::fs::write(
        path(dir, "main.csv"),
        "ref,val,package,x,y,rot,side\nR1,10k,R_0603,10,20,0,top\nFID1,mark,Fiducial,1,1,0,top\nFID2,mark,Fiducial,9,1,0,top\n",
    )
    .unwrap();
    let panel = path(dir, "panel.json");
    std::fs::write(
        &panel,
        r#"{
  "schema_version": 1,
  "boards": { "main": "main.csv" },
  "panels": { "p": {
    "children": [
      { "id": "A", "board": "main", "x": 0, "y": 0, "rotation": 0 },
      { "id": "B", "board": "main", "x": 60, "y": 0, "rotation": 0 },
      { "id": "C", "board": "main", "x": 120, "y": 0, "rotation": 0, "x_out": true } ],
    "fiducials": [ { "id": "f1", "x": -5, "y": -5 }, { "id": "f2", "x": 185, "y": -5 } ] } },
  "root": "p"
}"#,
    )
    .unwrap();
    // the panel is 1 to the right and 2 up; the board A is 0.5 to the right and 0.5 up
    let marks = path(dir, "marks.csv");
    std::fs::write(
        &marks,
        "id,x,y\nf1,-4,-3\nf2,186,-3\nA/FID1,1.5,1.5\nA/FID2,9.5,1.5\n",
    )
    .unwrap();
    (panel, marks)
}

#[test]
fn a_board_marked_as_bad_is_not_planned_and_the_fiducial_parts_are_marks_of_two_levels() {
    let dir = scratch("two_levels");
    let (panel, marks) = two_level_files(&dir);
    let (plain, moved) = (path(&dir, "plain.json"), path(&dir, "moved.json"));
    let (feeders, config) = (example("feeders.csv"), example("machine.cfg"));
    let o = run(&[
        "schedule", &feeders, &plain, "--panel", &panel, "--config", &config,
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    assert!(
        text.contains("x-out: C is marked as bad and is left out"),
        "{text}"
    );
    assert!(
        text.contains("fiducial parts: 4 are marks of their boards and are not placed (A/FID1, A/FID2, B/FID1, B/FID2)"),
        "{text}"
    );
    let a = plan_of(&plain);
    let references: Vec<&str> = a["placements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["reference"].as_str().unwrap())
        .collect();
    assert_eq!(references.len(), 2, "{references:?}");
    assert!(references.contains(&"A/R1") && references.contains(&"B/R1"));
    let o = run(&[
        "schedule",
        &feeders,
        &moved,
        "--panel",
        &panel,
        "--config",
        &config,
        "--fiducials",
        &marks,
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    assert!(
        text.contains("fiducials: 2 marks, similarity fit"),
        "{text}"
    );
    assert!(
        text.contains("fiducials of A: 2 marks, similarity fit"),
        "{text}"
    );
    let b = plan_of(&moved);
    // A/R1 at (10, 20) is moved by the fit of its board, (0.5, 0.5); B/R1 at (70, 20) by the fit of the panel, (1, 2)
    for (reference, dx, dy) in [("A/R1", 0.5, 0.5), ("B/R1", 1.0, 2.0)] {
        let (p, q) = (
            &placement(&a, reference)["place"],
            &placement(&b, reference)["place"],
        );
        let d = |k: &str| q[k].as_f64().unwrap() - p[k].as_f64().unwrap();
        assert!((d("x") - dx).abs() < 1e-6, "{reference}: {}", d("x"));
        assert!((d("y") - dy).abs() < 1e-6, "{reference}: {}", d("y"));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn x_out_on_the_command_line_names_boards_and_a_wrong_name_is_refused() {
    let dir = scratch("x_out_option");
    let (panel, _) = two_level_files(&dir);
    let plan = path(&dir, "plan.json");
    let (feeders, config) = (example("feeders.csv"), example("machine.cfg"));
    let o = run(&[
        "schedule", &feeders, &plan, "--panel", &panel, "--config", &config, "--x-out", "A",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("x-out: A is marked as bad"), "{}", out(&o));
    assert!(out(&o).contains("x-out: C is marked as bad"), "{}", out(&o));
    let v = plan_of(&plan);
    assert_eq!(v["placements"].as_array().unwrap().len(), 1);
    assert_eq!(v["placements"][0]["reference"], "B/R1");
    // a name that no board has
    let o = run(&[
        "schedule", &feeders, &plan, "--panel", &panel, "--config", &config, "--x-out", "Z",
    ]);
    assert!(!o.status.success());
    assert!(err(&o).contains("PHOENIX-PNP-042"), "{}", err(&o));
    // every board out
    let o = run(&[
        "schedule", &feeders, &plan, "--panel", &panel, "--config", &config, "--x-out", "A,B",
    ]);
    assert!(!o.status.success());
    assert!(err(&o).contains("PHOENIX-PNP-043"), "{}", err(&o));
    // the option names boards of a panel
    let o = run(&[
        "schedule",
        &feeders,
        &plan,
        &example("board.pos"),
        "--config",
        &config,
        "--x-out",
        "A",
    ]);
    assert!(!o.status.success());
    assert!(err(&o).contains("--x-out needs --panel"), "{}", err(&o));
    let _ = std::fs::remove_dir_all(&dir);
}
