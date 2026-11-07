//! Machines with more than one head: the lines of the configuration, what each head reaches, how the parts are shared out, the clock that makes a head wait for a feeder another head is
//! using, and the plan of `schedule` with two or more heads.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::{HeadSpec, MachineConfig};
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::heads::{plan_heads, reaches, schedule, HeadsPlan};
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options, PickTime};
use std::path::PathBuf;
use std::process::{Command, Output};

const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nR3,10k,R,30,10,180,top\nC1,100n,C,10,20,0,top\nC2,100n,C,20,20,0,top\nU1,MCU,Q,40,15,0,top\n";
const FEEDERS: &str =
    "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\n";
const ONE_KIND: &str = "ref,val,package,x,y,rot,side\nP1,10k,R,10,5,0,top\nP2,10k,R,140,5,0,top\nP3,10k,R,80,5,0,top\n";
const ONE_FEEDER: &str = "id,value,package,x,y,count\nF,10k,R,70,0,10\n";

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn pick(feeder: &str, at_s: f64, busy_s: f64) -> PickTime {
    PickTime {
        feeder: feeder.to_string(),
        at_s,
        busy_s,
    }
}

fn plan(
    parts: &str,
    feeders: &str,
    cfg: &str,
) -> Result<HeadsPlan, eh3d_phoenix_pnp::error::PnpError> {
    let cfg = MachineConfig::parse(cfg).unwrap();
    let parts = from_csv(parts).unwrap();
    let feeders = feeding::from_csv(feeders).unwrap();
    let picks = assign(&parts, &feeders, Some(Side::Top));
    plan_heads(
        &parts,
        &feeders,
        &picks,
        &Catalog::new(),
        &cfg,
        &Options::default(),
    )
}

fn references(h: &HeadsPlan, head: usize) -> Vec<String> {
    h.heads[head]
        .plan
        .plan
        .placements
        .iter()
        .map(|p| p.reference.clone())
        .collect()
}

// ---- the lines of the configuration

#[test]
fn a_head_line_has_a_name_nozzles_and_optional_fields() {
    let c = MachineConfig::parse("head = left : 2\nhead = right : 1 : 60.5 : -3").unwrap();
    assert_eq!(c.heads.len(), 2);
    assert_eq!(
        c.heads[0],
        HeadSpec {
            name: "left".into(),
            ..HeadSpec::implicit(2)
        }
    );
    assert_eq!(c.heads[1].nozzles, 1);
    assert!(near(c.heads[1].offset_x_mm, 60.5) && near(c.heads[1].offset_y_mm, -3.0));
    // without heads the machine has one, with the nozzles of the configuration
    let none = MachineConfig::parse("nozzles = 3").unwrap();
    assert_eq!(none.head_list(), vec![HeadSpec::implicit(3)]);
    assert_eq!(c.head_list(), c.heads);
}

#[test]
fn a_bad_head_line_is_refused() {
    for bad in [
        "head = a",
        "head = : 2",
        "head = a b : 2",
        "head = a : x",
        "head = a : 2 : 0 : 0 : 0 : 0 : 0 : 0 : 1 : extra",
        "head = a : 2 : left",
        "head = a : 1 : 0 : 0 : 0 : 0 : 0 : 0.0005",
    ] {
        let e = MachineConfig::parse(bad).unwrap_err();
        assert!(
            matches!(e.code, Code::ConfigBadValue | Code::ConfigOutOfRange),
            "{bad}: {e}"
        );
    }
    for (bad, code) in [
        ("head = a : 0", Code::ConfigOutOfRange),
        ("head = a : 7", Code::ConfigOutOfRange),
        ("head = a : 1 : 2001", Code::ConfigOutOfRange),
        ("head = a : 1 : 0 : -2001", Code::ConfigOutOfRange),
        ("head = a : 1 : 0 : 0 : 501", Code::ConfigOutOfRange),
        ("head = a : 1 : 0 : 0 : 0 : 0 : 181", Code::ConfigOutOfRange),
        (
            "head = a : 1 : 0 : 0 : 0 : 0 : 0 : 0 : 2.5",
            Code::ConfigOutOfRange,
        ),
        ("head = a : 1\nhead = a : 2", Code::ConfigBadValue),
        (
            "head = a : 1\nhead = b : 1\nhead = c : 1\nhead = d : 1\nhead = e : 1",
            Code::ConfigBadValue,
        ),
    ] {
        assert_eq!(MachineConfig::parse(bad).unwrap_err().code, code, "{bad}");
    }
    assert!(MachineConfig::parse("head = a : 1\nhead = b : 1\nhead = c : 1\nhead = d : 1").is_ok());
}

// ---- the clock

#[test]
fn a_head_that_comes_to_a_busy_feeder_waits_and_everything_after_it_moves() {
    // head 0 takes F1 from 1 s to 3 s; head 1 arrives at 2 s and waits until 3 s, so it is done one second later than alone
    let a = [pick("F1", 1.0, 2.0), pick("F2", 5.0, 1.0)];
    let b = [pick("F1", 2.0, 2.0)];
    let done = schedule(&[(10.0, &a[..]), (8.0, &b[..])]);
    assert_eq!(done, vec![(10.0, 0.0), (9.0, 1.0)]);
}

#[test]
fn the_wait_of_one_pick_can_absorb_the_next() {
    // head 0 holds F1 from 0 to 4; head 1 arrives at 1 and waits until 4 (3 s), takes F1 until 5, and its next pick (at 3 s alone, 6 s now) finds F1 free
    let a = [pick("F1", 0.0, 4.0)];
    let b = [pick("F1", 1.0, 1.0), pick("F1", 3.0, 1.0)];
    let done = schedule(&[(4.0, &a[..]), (5.0, &b[..])]);
    assert_eq!(done, vec![(4.0, 0.0), (8.0, 3.0)]);
}

#[test]
fn the_first_head_goes_first_when_they_arrive_together_and_other_feeders_do_not_wait() {
    let a = [pick("F", 1.0, 2.0)];
    let b = [pick("F", 1.0, 2.0)];
    assert_eq!(
        schedule(&[(5.0, &a[..]), (5.0, &b[..])]),
        vec![(5.0, 0.0), (7.0, 2.0)]
    );
    let c = [pick("G", 1.0, 2.0)];
    assert_eq!(
        schedule(&[(5.0, &a[..]), (6.0, &c[..])]),
        vec![(5.0, 0.0), (6.0, 0.0)]
    );
    // nothing at all, and a head with no pick
    assert!(schedule(&[]).is_empty());
    assert_eq!(schedule(&[(0.0, &[][..])]), vec![(0.0, 0.0)]);
}

// ---- what a head reaches

#[test]
fn a_head_reaches_what_the_carriage_can_reach_less_its_offset() {
    let cfg = MachineConfig::parse(
        "limit_x_min_mm = -5\nlimit_x_max_mm = 100\nlimit_y_min_mm = -100\nlimit_y_max_mm = 100",
    )
    .unwrap();
    let left = HeadSpec::implicit(1);
    let right = HeadSpec {
        offset_x_mm: 60.0,
        ..HeadSpec::implicit(1)
    };
    // the limits are on the carriage, the places on the board
    assert!(reaches(&cfg, &left, (-5.0, 0.0)) && reaches(&cfg, &left, (100.0, 100.0)));
    assert!(!reaches(&cfg, &left, (100.001, 0.0)) && !reaches(&cfg, &left, (0.0, -100.001)));
    assert!(reaches(&cfg, &right, (55.0, 0.0)) && reaches(&cfg, &right, (160.0, 0.0)));
    assert!(!reaches(&cfg, &right, (54.9, 0.0)) && !reaches(&cfg, &right, (160.1, 0.0)));
}

const TWO_ENDS: &str = "limit_x_min_mm = -5\nlimit_x_max_mm = 100\nlimit_y_min_mm = -100\nlimit_y_max_mm = 100\nhead = left : 1 : 0 : 0\nhead = right : 1 : 60 : 0\n";

#[test]
fn each_part_goes_to_a_head_that_reaches_it_and_is_given_back_in_board_coordinates() {
    let h = plan(ONE_KIND, ONE_FEEDER, TWO_ENDS).unwrap();
    assert_eq!(h.parts(), 3);
    // P1 is at x 10, out of reach of the head at 60; P2 at x 140, out of reach of the head at 0
    assert!(references(&h, 0).contains(&"P1".to_string()));
    assert!(references(&h, 1).contains(&"P2".to_string()));
    let mut all: Vec<String> = references(&h, 0);
    all.extend(references(&h, 1));
    all.sort();
    assert_eq!(all, ["P1", "P2", "P3"]);
    // the plan of a head is in the coordinates of its carriage: P2 is at 140 on the board and the right head at 60 puts its nozzle there with the carriage at 80
    let right = &h.heads[1];
    let i = references(&h, 1).iter().position(|r| r == "P2").unwrap();
    assert!(near(right.plan.plan.placements[i].x, 80.0));
    assert_eq!(right.board_xy(i), Some((140.0, 5.0)));
    assert_eq!(right.board_xy(99), None);
    let left = &h.heads[0];
    let i = references(&h, 0).iter().position(|r| r == "P1").unwrap();
    assert_eq!(left.board_xy(i), Some((10.0, 5.0)));
}

#[test]
fn a_part_that_no_head_reaches_is_an_error_that_names_it() {
    let e = plan(
        ONE_KIND,
        ONE_FEEDER,
        "limit_x_min_mm = -5\nlimit_x_max_mm = 100\nlimit_y_min_mm = -100\nlimit_y_max_mm = 100\nhead = left : 1\nhead = right : 1 : 10",
    )
    .unwrap_err();
    assert_eq!(e.code, Code::HeadUnreachable);
    assert_eq!(e.code.label(), "PHOENIX-PNP-048");
    assert!(e.detail.contains("P2"), "{}", e.detail);
    // the feeder counts too: a pick out of reach of every head is as bad as a place
    let far = "id,value,package,x,y,count\nF,10k,R,500,0,10\n";
    let e = plan(
        ONE_KIND,
        far,
        "limit_x_max_mm = 100
head = a : 1",
    )
    .unwrap_err();
    assert_eq!(e.code, Code::HeadUnreachable);
}

// ---- the plan

#[test]
fn a_machine_with_one_head_plans_as_it_always_did() {
    for cfg in ["nozzles = 2", "nozzles = 1", "head = only : 2\nnozzles = 2"] {
        let h = plan(PARTS, FEEDERS, cfg).unwrap();
        let c = MachineConfig::parse(cfg).unwrap();
        let parts = from_csv(PARTS).unwrap();
        let feeders = feeding::from_csv(FEEDERS).unwrap();
        let picks = assign(&parts, &feeders, Some(Side::Top));
        let alone = plan_advanced(
            &parts,
            &feeders,
            &picks,
            &Catalog::new(),
            &c,
            &Options::default(),
        )
        .unwrap();
        assert_eq!(h.heads.len(), 1);
        assert_eq!(h.heads[0].plan, alone, "{cfg}");
        assert!(near(h.seconds, alone.plan.seconds) && near(h.wait_s, 0.0));
    }
}

#[test]
fn two_heads_share_out_the_parts_once_and_finish_before_one_head_would() {
    let one = plan(PARTS, FEEDERS, "head = a : 1").unwrap();
    let two = plan(PARTS, FEEDERS, "head = a : 1\nhead = b : 1").unwrap();
    assert_eq!(two.parts(), 6);
    let mut all = references(&two, 0);
    all.extend(references(&two, 1));
    all.sort();
    assert_eq!(all, ["C1", "C2", "R1", "R2", "R3", "U1"]);
    assert!(!references(&two, 0).is_empty() && !references(&two, 1).is_empty());
    assert!(two.seconds < one.seconds, "{} {}", two.seconds, one.seconds);
    // the clock: each head is done at the seconds of its plan plus its waits, and the machine at the last of them
    for h in &two.heads {
        assert!(near(h.finish_s, h.plan.plan.seconds + h.wait_s));
    }
    let last = two.heads.iter().map(|h| h.finish_s).fold(0.0, f64::max);
    assert!(near(two.seconds, last));
    assert!(near(
        two.wait_s,
        two.heads.iter().map(|h| h.wait_s).sum::<f64>()
    ));
    // the same files give the same plan
    assert_eq!(
        two,
        plan(PARTS, FEEDERS, "head = a : 1\nhead = b : 1").unwrap()
    );
}

#[test]
fn heads_that_take_from_the_same_feeder_wait_for_each_other() {
    let all_resistors = "ref,val,package,x,y,rot,side\nA,10k,R,10,10,0,top\nB,10k,R,20,10,0,top\nC,10k,R,30,10,0,top\nD,10k,R,40,10,0,top\n";
    let one_feeder = "id,value,package,x,y,count\nF,10k,R,-20,10,10\n";
    let h = plan(all_resistors, one_feeder, "head = a : 1\nhead = b : 1").unwrap();
    assert_eq!(h.parts(), 4);
    assert!(h.wait_s > 0.0, "{}", h.wait_s);
    let alone_longest = h
        .heads
        .iter()
        .map(|x| x.plan.plan.seconds)
        .fold(0.0, f64::max);
    assert!(h.seconds > alone_longest);
}

#[test]
fn the_json_of_the_heads_lists_what_each_head_does() {
    let h = plan(ONE_KIND, ONE_FEEDER, TWO_ENDS).unwrap();
    let v = h.to_value();
    let heads = v["heads"].as_array().unwrap();
    assert_eq!(heads.len(), 2);
    assert_eq!(heads[1]["name"], "right");
    assert_eq!(heads[1]["offset"]["x"], 60.0);
    let total: u64 = heads.iter().map(|x| x["parts"].as_u64().unwrap()).sum();
    assert_eq!(total, 3);
    // the places are in the coordinates of the board
    let p2 = heads
        .iter()
        .flat_map(|x| x["placements"].as_array().unwrap())
        .find(|p| p["reference"] == "P2")
        .unwrap();
    assert_eq!(p2["x"], 140.0);
    assert!(near(v["seconds"].as_f64().unwrap(), h.seconds));
    let text = h.summary();
    assert!(text.contains("head left: "), "{text}");
    assert!(text.contains("head right: "), "{text}");
    assert!(text.contains("heads together: 3 parts in about "), "{text}");
}

// ---- the command line

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eh3d_pnp_heads_{}_{name}", std::process::id()));
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

fn schedule_with(name: &str, config: &str) -> (Output, String) {
    let dir = scratch(name);
    let p = |f: &str| dir.join(f).to_string_lossy().into_owned();
    std::fs::write(p("parts.csv"), PARTS).unwrap();
    std::fs::write(p("feeders.csv"), FEEDERS).unwrap();
    std::fs::write(p("machine.cfg"), config).unwrap();
    let o = run(&[
        "schedule",
        &p("parts.csv"),
        &p("feeders.csv"),
        &p("plan.json"),
        "--config",
        &p("machine.cfg"),
    ]);
    let json = std::fs::read_to_string(p("plan.json")).unwrap_or_default();
    (o, json)
}

#[test]
fn schedule_with_two_heads_tells_what_each_one_does_and_writes_it_in_the_plan() {
    let (o, json) = schedule_with("two", "head = left : 1\nhead = right : 1 : 30 : 0\n");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let text = out(&o);
    assert!(text.contains("head left: "), "{text}");
    assert!(text.contains("heads together: 6 parts"), "{text}");
    let doc: serde_json::Value = serde_json::from_str(&json).unwrap();
    // the plan of one head is still there, as it always was, and the heads are a key of their own
    assert_eq!(doc["placements"].as_array().unwrap().len(), 6);
    assert_eq!(doc["heads"]["heads"].as_array().unwrap().len(), 2);
}

#[test]
fn schedule_with_one_head_or_none_writes_the_plan_it_always_wrote() {
    for (name, config) in [("none", ""), ("one", "head = main : 1\n")] {
        let (o, json) = schedule_with(name, config);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        assert!(!out(&o).contains("heads together"), "{}", out(&o));
        let doc: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(doc.get("heads").is_none(), "{name}");
    }
}

#[test]
fn schedule_names_the_part_that_no_head_reaches_in_the_language_asked_for() {
    let dir = scratch("far");
    let p = |f: &str| dir.join(f).to_string_lossy().into_owned();
    std::fs::write(p("parts.csv"), PARTS).unwrap();
    std::fs::write(p("feeders.csv"), FEEDERS).unwrap();
    std::fs::write(
        p("machine.cfg"),
        "limit_x_max_mm = 30\nhead = a : 1\nhead = b : 1 : 5\n",
    )
    .unwrap();
    let o = run(&[
        "schedule",
        &p("parts.csv"),
        &p("feeders.csv"),
        &p("plan.json"),
        "--config",
        &p("machine.cfg"),
        "--lang",
        "es",
        "--partial",
    ]);
    assert_eq!(o.status.code(), Some(1));
    let e = String::from_utf8_lossy(&o.stderr).into_owned();
    assert!(e.contains("PHOENIX-PNP-048"), "{e}");
    assert!(
        e.contains("Ninguna cabeza de la máquina alcanza la pieza"),
        "{e}"
    );
    assert!(e.contains("U1"), "{e}");
}

#[test]
fn the_heads_keep_the_parts_of_a_tip_together_when_they_can() {
    let rules = "tip_compat = R : ta
tip_compat = C : tb
";
    let one = plan(PARTS, FEEDERS, &format!("{rules}head = a : 1")).unwrap();
    let two = plan(
        PARTS,
        FEEDERS,
        &format!(
            "{rules}head = a : 1
head = b : 1"
        ),
    )
    .unwrap();
    // three tips on one head are two changes; shared out, no head changes more often than that and the heads together do not change more either
    assert_eq!(one.heads[0].plan.tip_changes, 2);
    let changes: usize = two.heads.iter().map(|h| h.plan.tip_changes).sum();
    assert!(changes <= 2, "{changes}");
    assert_eq!(two.parts(), 6);
}
