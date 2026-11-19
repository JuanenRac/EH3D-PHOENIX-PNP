//! The odds that a board goes through with no part in error: the arithmetic of the retries, the rules by package, the file of rules read with care, and `failure-odds`.

use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::failodds::{
    analyse, parse_rules, stage_fail, stage_retries, MAX_RETRIES, MAX_RULES,
};
use eh3d_phoenix_pnp::feeding::{assign, from_csv as feeders_from_csv};
use eh3d_phoenix_pnp::parts::parse;
use std::path::PathBuf;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_phoenix-pnp");

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
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
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_odds_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn example(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join(name),
    )
    .unwrap()
}

fn odds(cfg: &str, rules: &str) -> eh3d_phoenix_pnp::failodds::Odds {
    let parts = parse(&example("board.pos")).unwrap();
    let feeders = feeders_from_csv(&example("feeders.csv")).unwrap();
    let picks = assign(&parts, &feeders, None);
    let rules = if rules.is_empty() {
        Vec::new()
    } else {
        parse_rules(rules).unwrap()
    };
    analyse(
        &parts,
        &feeders,
        &picks,
        &MachineConfig::parse(cfg).unwrap(),
        &rules,
    )
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} {b}");
}

#[test]
fn a_stage_that_may_be_tried_again_ends_in_error_only_when_every_try_fails() {
    close(stage_fail(0.1, 0), 0.1);
    close(stage_fail(0.1, 2), 0.001);
    close(stage_fail(0.0, 5), 0.0);
    close(stage_fail(1.0, 5), 1.0);
    // a chance outside 0 to 1 is held to it
    close(stage_fail(-1.0, 1), 0.0);
    close(stage_fail(2.0, 1), 1.0);
    close(stage_retries(0.1, 0), 0.0);
    close(stage_retries(0.1, 2), 0.1 + 0.01);
    close(stage_retries(0.5, 3), 0.5 + 0.25 + 0.125);
    close(stage_retries(1.0, 4), 4.0);
}

#[test]
fn a_machine_that_never_fails_goes_through_every_time() {
    let o = odds("", "");
    assert_eq!(
        (o.parts, o.failed, o.board_ok, o.retries, o.retry_s),
        (7, 0.0, 1.0, 0.0, 0.0)
    );
    assert!(o.without_feeder.is_empty());
    assert_eq!(o.rows.len(), 4);
}

#[test]
fn the_chance_of_a_board_is_the_product_of_the_chances_of_its_parts() {
    // a pick fails 10% of the time and is tried 3 times (pick_retries = 2): 0.1% of the parts end in error
    let o = odds("sim_pick_fail_rate = 0.1\npick_retries = 2\n", "");
    close(o.failed, 7.0 * 0.001);
    close(o.board_ok, 0.999f64.powi(7));
    close(o.retries, 7.0 * (0.1 + 0.01));
    // the time of the retries is the pick time (0.4 s by default) for each
    close(o.retry_s, 7.0 * (0.1 + 0.01) * 0.4);
    let r_0603 = o.rows.iter().find(|r| r.package == "R_0603").unwrap();
    assert_eq!((r_0603.parts, r_0603.allowed), (3, (1, 2, 1)));
    close(r_0603.fail, 0.001);
    close(r_0603.failed, 3.0 * 0.001);
}

#[test]
fn the_three_stages_and_the_causes_of_a_pick_add_up() {
    let o = odds(
        "sim_feed_fail_rate = 0.2\nfeed_retries = 1\nsim_pick_fail_rate = 0.1\nsim_feeder_fail = F3 : 0.5\npick_retries = 0\nsim_place_fail_rate = 0.3\nplace_retries = 0\n",
        "",
    );
    // the MCU comes from F3: a pick fails with 1 - 0.9 * 0.5 = 0.55, with no retry
    let any = |c: &[f64]| 1.0 - c.iter().fold(1.0, |a, c| a * (1.0 - c));
    let qfn = o.rows.iter().find(|r| r.package == "QFN-32").unwrap();
    close(qfn.fail, any(&[0.04, 0.55, 0.3]));
    let led = o.rows.iter().find(|r| r.package == "LED_0603").unwrap();
    close(led.fail, any(&[0.04, 0.1, 0.3]));
    // the worst package comes first
    assert_eq!(o.rows[0].package, "R_0603");
    assert!(o.rows.windows(2).all(|w| w[0].failed >= w[1].failed));
}

#[test]
fn the_failure_of_a_package_comes_from_its_rule_and_the_first_rule_that_matches_wins() {
    let cfg = "sim_part_fail = QFN-* : 0.5 : 0.0\npick_retries = 0\n";
    let none = odds(cfg, "");
    close(
        none.rows
            .iter()
            .find(|r| r.package == "QFN-32")
            .unwrap()
            .fail,
        0.5,
    );
    let rules = "pattern,pick_retries\nQFN-*,3\n*,0\n";
    let with = odds(cfg, rules);
    let qfn = with.rows.iter().find(|r| r.package == "QFN-32").unwrap();
    close(qfn.fail, 0.5f64.powi(4));
    assert_eq!(qfn.allowed.1, 3);
    // the package of another rule keeps the setting of the file of rules `*`
    assert_eq!(
        with.rows
            .iter()
            .find(|r| r.package == "R_0603")
            .unwrap()
            .allowed
            .1,
        0
    );
    // an empty cell keeps the setting
    let keep = odds(
        "pick_retries = 4\nsim_pick_fail_rate = 0.5\n",
        "pattern,feed_retries,pick_retries\nR_*,2,\n",
    );
    let r = keep.rows.iter().find(|r| r.package == "R_0603").unwrap();
    assert_eq!(r.allowed, (2, 4, 1));
}

#[test]
fn more_retries_never_make_a_board_worse() {
    let mut last = 0.0;
    for retries in 0..=6 {
        let o = odds(&format!("max_placement_attempts = 10\nsim_pick_fail_rate = 0.3\nsim_place_fail_rate = 0.1\nsim_feed_fail_rate = 0.1\npick_retries = {retries}\nplace_retries = {retries}\nfeed_retries = {retries}\n"), "");
        assert!(o.board_ok >= last, "{retries}: {} {last}", o.board_ok);
        assert!((0.0..=1.0).contains(&o.board_ok));
        last = o.board_ok;
    }
    assert!(last > 0.99);
}

#[test]
fn a_part_that_always_fails_makes_the_board_certain_to_fail_and_many_parts_do_not_underflow() {
    let o = odds("sim_pick_fail_rate = 1\n", "");
    assert_eq!(o.board_ok, 0.0);
    assert_eq!(o.failed, 7.0);
    // a thousand parts that each fail one time in a thousand: the chance is e^-1, not 0
    let mut pos = String::from("ref,val,package,posx,posy,rot,side\n");
    for i in 0..1000 {
        pos.push_str(&format!("R{i},1k,P,{i},0,0,top\n"));
    }
    let parts = parse(&pos).unwrap();
    let feeders = feeders_from_csv("id,value,package,x,y,count\nF1,1k,P,0,0,5000\n").unwrap();
    let picks = assign(&parts, &feeders, None);
    let cfg = MachineConfig::parse("sim_pick_fail_rate = 0.001\npick_retries = 0\n").unwrap();
    let o = analyse(&parts, &feeders, &picks, &cfg, &[]);
    assert!((o.board_ok - (0.999f64.powi(1000))).abs() < 1e-9);
    assert!(o.board_ok > 0.36 && o.board_ok < 0.37);
}

#[test]
fn a_part_with_no_feeder_is_told_and_not_counted() {
    let parts = parse(&example("board.pos")).unwrap();
    let feeders = feeders_from_csv("id,value,package,x,y,count\nF1,10k,R_0603,0,0,100\n").unwrap();
    let picks = assign(&parts, &feeders, None);
    let o = analyse(&parts, &feeders, &picks, &MachineConfig::default(), &[]);
    assert_eq!((o.parts, o.without_feeder.len()), (3, 4));
    assert!(o.text().contains("no feeder holds: C1 C2 D1 U1"));
}

#[test]
fn the_file_of_rules_is_read_with_its_columns_in_any_order() {
    let r = parse_rules("PLACE_RETRIES;Pattern\n2;QFN-*\n").unwrap();
    assert_eq!(
        (r[0].pattern.as_str(), r[0].feed, r[0].pick, r[0].place),
        ("QFN-*", None, None, Some(2))
    );
    let r = parse_rules(&example("failure_rules.csv")).unwrap();
    assert_eq!(r.len(), 3);
    assert_eq!((r[0].feed, r[0].pick, r[0].place), (None, Some(3), Some(2)));
    assert_eq!((r[2].feed, r[2].pick, r[2].place), (Some(0), None, Some(0)));
}

#[test]
fn a_file_of_rules_that_cannot_be_trusted_is_refused_with_its_row() {
    for (text, what) in [
        ("", "empty"),
        ("pick_retries\n1\n", "needs the column pattern"),
        ("pattern\nQFN-*\n", "at least one of"),
        ("pattern,pick_retries\n,1\n", "row 2: the pattern is empty"),
        (
            "pattern,pick_retries\nQFN-*,x\n",
            "row 2: 'x' is not a whole number",
        ),
        ("pattern,pick_retries\nQFN-*,21\n", "row 2"),
        ("pattern,pick_retries\nQFN-*,-1\n", "row 2"),
        ("pattern,pick_retries\nA,1\nB,1.5\n", "row 3"),
    ] {
        let e = parse_rules(text).unwrap_err();
        assert!(e.to_string().contains(what), "{text:?}: {e}");
    }
    assert!(parse_rules(&format!("pattern,pick_retries\nA,{MAX_RETRIES}\n")).is_ok());
    let mut many = String::from("pattern,pick_retries\n");
    for i in 0..=MAX_RULES {
        many.push_str(&format!("P{i},1\n"));
    }
    assert!(parse_rules(&many).is_err());
}

#[test]
fn the_csv_has_a_row_for_each_package() {
    let o = odds("sim_pick_fail_rate = 0.1\npick_retries = 2\n", "");
    let csv = o.csv();
    assert_eq!(csv.lines().next().unwrap(), "package,parts,feed_retries,pick_retries,place_retries,fail_percent,failed_per_board,retries_per_part");
    assert_eq!(csv.lines().count(), 5);
    assert!(
        csv.contains("R_0603,3,1,2,1,0.100000,0.003000,0.110000"),
        "{csv}"
    );
}

#[test]
fn the_command_prints_the_odds_and_writes_the_table() {
    let dir = temp("cmd");
    let cfg = dir.join("m.cfg");
    std::fs::write(&cfg, "sim_pick_fail_rate = 0.1\npick_retries = 2\n").unwrap();
    let c = cfg.to_string_lossy().into_owned();
    let file = dir.join("odds.csv").to_string_lossy().into_owned();
    let o = run(&[
        "failure-odds",
        "examples/board.pos",
        "examples/feeders.csv",
        "--config",
        &c,
        "--out",
        &file,
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    assert!(
        text.contains(
            "R_0603: 3 parts, retries 1/2/1 (feed/pick/place), 0.1000% of the parts end in error"
        ),
        "{text}"
    );
    assert!(
        text.contains("99.302% of the boards go through with none"),
        "{text}"
    );
    assert!(std::fs::read_to_string(&file)
        .unwrap()
        .contains("R_0603,3,1,2,1,0.100000"));
    let again = run(&[
        "failure-odds",
        "examples/board.pos",
        "examples/feeders.csv",
        "--config",
        &c,
        "--out",
        &file,
    ]);
    assert_eq!(again.status.code(), Some(1));
    assert!(err(&again).contains("PHOENIX-PNP-040"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_command_takes_the_rules_and_fails_under_the_chance_that_is_asked_for() {
    let dir = temp("rules");
    let cfg = dir.join("m.cfg");
    std::fs::write(&cfg, "sim_pick_fail_rate = 0.3\npick_retries = 0\n").unwrap();
    let c = cfg.to_string_lossy().into_owned();
    let base = [
        "failure-odds",
        "examples/board.pos",
        "examples/feeders.csv",
        "--config",
        c.as_str(),
    ];
    let o = run(&base);
    assert!(o.status.success(), "no minimum: it never fails");
    let o = run(&[&base[..], &["--min-ok", "50"]].concat());
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("under the 50% asked for"), "{}", err(&o));
    // retries for every package by a rule bring it above 50%
    let rules = dir.join("rules.csv");
    std::fs::write(&rules, "pattern,pick_retries\n*,5\n").unwrap();
    let r = rules.to_string_lossy().into_owned();
    let o = run(&[&base[..], &["--rules", r.as_str(), "--min-ok", "50"]].concat());
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("retries 1/5/1"), "{}", out(&o));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_command_checks_its_arguments() {
    for args in [
        vec!["failure-odds", "examples/board.pos"],
        vec![
            "failure-odds",
            "examples/board.pos",
            "examples/feeders.csv",
            "--min-ok",
            "101",
        ],
        vec![
            "failure-odds",
            "examples/board.pos",
            "examples/feeders.csv",
            "--min-ok",
            "-1",
        ],
        vec![
            "failure-odds",
            "examples/board.pos",
            "examples/feeders.csv",
            "--rules",
            "examples/nope.csv",
        ],
        vec![
            "failure-odds",
            "examples/board.pos",
            "examples/feeders.csv",
            "--rules",
            "examples/feeders.csv",
        ],
        vec![
            "failure-odds",
            "examples/board.pos",
            "examples/feeders.csv",
            "extra",
        ],
    ] {
        let o = run(&args);
        assert_eq!(o.status.code(), Some(1), "{args:?}");
        assert!(err(&o).starts_with("error:"), "{args:?}");
    }
}
