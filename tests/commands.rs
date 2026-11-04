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
