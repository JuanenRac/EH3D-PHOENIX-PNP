//! Two views of a simulated job read from its journal: how busy each nozzle and each feeder was and where the machine waited, and the light a signal tower would show at each second.

use eh3d_phoenix_pnp::busy::{analyse, read_journal, Step};
use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::journal::{self, Row};
use eh3d_phoenix_pnp::tower::{self, Light};

fn step(
    n: usize,
    at: f64,
    took: f64,
    action: &str,
    feeder: &str,
    nozzle: usize,
    result: &str,
) -> Step {
    Step {
        step: n,
        at_s: at,
        took_s: took,
        action: action.into(),
        reference: format!("R{n}"),
        feeder: feeder.into(),
        nozzle,
        result: result.into(),
        status: "placed".into(),
    }
}

/// Four parts with two nozzles: the picks take 1 s, the places 2 s, and there is a wait of 5 s before the last pick.
fn job() -> Vec<Step> {
    vec![
        step(1, 0.0, 1.0, "pick", "F1", 1, "ok"),
        step(2, 1.0, 1.0, "pick", "F2", 2, "ok"),
        step(3, 2.0, 2.0, "place", "F1", 1, "ok"),
        step(4, 4.0, 2.0, "place", "F2", 2, "ok"),
        step(5, 11.0, 1.0, "pick", "F1", 1, "ok"),
    ]
}

fn csv_of(steps: &[Step]) -> String {
    let rows: Vec<Row> = steps
        .iter()
        .map(|s| Row {
            step: s.step,
            at_s: s.at_s,
            took_s: s.took_s,
            action: if s.action == "pick" { "pick" } else { "place" },
            reference: s.reference.clone(),
            feeder: s.feeder.clone(),
            nozzle: s.nozzle,
            result: s.result.clone(),
            status: "placed",
        })
        .collect();
    journal::csv(&rows)
}

#[test]
fn the_journal_that_the_simulator_writes_is_read_back() {
    let steps = job();
    let back = read_journal(&csv_of(&steps)).unwrap();
    assert_eq!(back.len(), steps.len());
    for (a, b) in steps.iter().zip(&back) {
        assert_eq!(
            (a.step, a.nozzle, &a.action, &a.feeder),
            (b.step, b.nozzle, &b.action, &b.feeder)
        );
        assert!((a.at_s - b.at_s).abs() < 1e-3 && (a.took_s - b.took_s).abs() < 1e-3);
        assert_eq!(b.status, "placed");
    }
}

#[test]
fn the_columns_of_a_journal_are_found_by_name_in_any_order_and_the_status_may_be_missing() {
    let text =
        "nozzle,result,reference,feeder,action,took_s,at_s,step\n1,ok,R1,F1,pick,0.5,2.0,7\n";
    let s = read_journal(text).unwrap();
    assert_eq!(
        (s[0].step, s[0].nozzle, s[0].at_s, s[0].took_s),
        (7, 1, 2.0, 0.5)
    );
    assert_eq!(s[0].status, "");
}

#[test]
fn a_journal_that_makes_no_sense_is_an_error_with_its_line() {
    let head = "step,at_s,took_s,action,reference,feeder,nozzle,result,status\n";
    for (text, line, word) in [
        ("".to_string(), 1, "empty"),
        (head.to_string(), 1, "no steps"),
        ("step,at_s\n1,2\n".to_string(), 1, "took_s"),
        (format!("{head}1,x,1,pick,R1,F1,1,ok,placed\n"), 2, "at_s"),
        (
            format!("{head}1,0,-1,pick,R1,F1,1,ok,placed\n"),
            2,
            "took_s",
        ),
        (
            format!("{head}1,0,1,pick,R1,F1,one,ok,placed\n"),
            2,
            "nozzle",
        ),
        (
            format!("{head}1,0,1,pick,R1,F1,1,ok,placed\n2,0,NaN,pick,R2,F1,1,ok,placed\n"),
            3,
            "took_s",
        ),
    ] {
        let e = read_journal(&text).unwrap_err();
        assert_eq!(e.code, Code::ImportBad, "{text:?}");
        assert!(
            e.to_string().contains(&format!("line {line}")) && e.to_string().contains(word),
            "{text:?}: {e}"
        );
    }
}

#[test]
fn the_use_of_each_nozzle_and_feeder_is_the_seconds_of_its_steps_over_the_time_of_the_job() {
    let b = analyse(&job(), 5);
    assert!((b.total_s - 12.0).abs() < 1e-9);
    let n1 = b.nozzles.iter().find(|u| u.name == "1").unwrap();
    assert!((n1.busy_s - 4.0).abs() < 1e-9 && n1.steps == 3);
    assert!((n1.share - 4.0 / 12.0).abs() < 1e-9);
    let n2 = b.nozzles.iter().find(|u| u.name == "2").unwrap();
    assert!((n2.busy_s - 3.0).abs() < 1e-9);
    // only the picks load a feeder
    let f1 = b.feeders.iter().find(|u| u.name == "F1").unwrap();
    assert!((f1.busy_s - 2.0).abs() < 1e-9 && f1.steps == 2);
    assert!(b
        .nozzles
        .iter()
        .chain(&b.feeders)
        .all(|u| (0.0..=1.0).contains(&u.share)));
}

#[test]
fn the_waits_between_steps_are_listed_longest_first_and_counted() {
    let mut steps = job();
    steps.insert(3, step(9, 3.5, 0.5, "place", "F1", 1, "ok"));
    steps[4].at_s = 4.0;
    let b = analyse(&steps, 5);
    assert!(b.waiting_s > 0.0);
    assert_eq!(b.gaps[0].after_step, 4);
    assert!((b.gaps[0].length_s - 5.0).abs() < 1e-9);
    assert!(b.gaps.windows(2).all(|w| w[0].length_s >= w[1].length_s));
    let one = analyse(&steps, 1);
    assert_eq!(one.gaps.len(), 1);
    assert!((b.waiting_s - b.gaps.iter().map(|g| g.length_s).sum::<f64>()).abs() < 1e-9);
}

#[test]
fn a_step_that_did_not_end_well_is_counted_and_the_text_and_csv_say_what_they_hold() {
    let mut steps = job();
    steps[2].result = "PHOENIX-PNP-022".into();
    let b = analyse(&steps, 3);
    assert_eq!(b.failed, 1);
    let t = b.text();
    assert!(t.contains("12.000 s from the first step to the last"));
    assert!(t.contains("5.000 s of waiting between steps, 1 steps that did not end well"));
    assert!(t.contains("nozzle 1: 4.000 s in 3 steps, 33.3 % of the job"));
    assert!(t.contains("feeder F1: 2.000 s in 2 picks"));
    assert!(t.contains("the machine waited 5.000 s after step 4 (from second 6.000)"));
    let c = b.csv();
    assert!(c.starts_with("kind,name,busy_s,steps,share\n"));
    assert!(c.contains("nozzle,1,4.000,3,0.3333"));
    assert!(c.contains("feeder,F2,1.000,1,"));
    assert!(analyse(&[], 3).nozzles.is_empty());
}

#[test]
fn the_tower_is_green_while_the_machine_works_and_the_seconds_add_up() {
    let steps = vec![
        step(1, 0.0, 1.0, "pick", "F1", 1, "ok"),
        step(2, 1.0, 1.0, "pick", "F1", 1, "ok"),
        step(3, 2.0, 1.0, "pick", "F1", 1, "ok"),
    ];
    let t = tower::timeline(&steps, 2.0, 3.0);
    assert_eq!(t.len(), 1);
    assert_eq!((t[0].light, t[0].why), (Light::Green, "running"));
    assert!((t[0].from_s, t[0].to_s) == (0.0, 3.0));
    assert_eq!(tower::seconds(&t), [3.0, 0.0, 0.0]);
}

#[test]
fn the_tower_is_red_on_a_failure_and_yellow_on_a_slow_step_and_on_a_long_wait() {
    let mut steps = job();
    steps[0].took_s = 1.0;
    steps.push(step(6, 12.0, 9.0, "pick", "F1", 1, "ok"));
    steps.push(step(7, 21.0, 1.0, "pick", "F1", 1, "PHOENIX-PNP-022"));
    let t = tower::timeline(&steps, 2.0, 3.0);
    let find = |why: &str| t.iter().find(|i| i.why == why);
    let wait = find("waiting").unwrap();
    assert_eq!(
        (wait.light, wait.from_s, wait.to_s),
        (Light::Yellow, 6.0, 11.0)
    );
    let slow = find("slow step").unwrap();
    assert_eq!(
        (slow.light, slow.from_s, slow.to_s),
        (Light::Yellow, 12.0, 21.0)
    );
    let failed = find("failed").unwrap();
    assert_eq!(
        (failed.light, failed.from_s, failed.to_s),
        (Light::Red, 21.0, 22.0)
    );
    let s = tower::seconds(&t);
    assert!((s[0] + s[1] + s[2] - 22.0).abs() < 1e-9);
    assert!(t.windows(2).all(|w| w[0].to_s <= w[1].from_s + 1e-9));
}

#[test]
fn a_wait_under_the_limit_is_not_yellow_and_the_slow_factor_is_at_least_one() {
    let steps = vec![
        step(1, 0.0, 1.0, "pick", "F1", 1, "ok"),
        step(2, 2.0, 1.0, "pick", "F1", 1, "ok"),
    ];
    assert!(tower::timeline(&steps, 2.0, 3.0)
        .iter()
        .all(|i| i.light == Light::Green));
    assert!(tower::timeline(&steps, 0.5, 3.0)
        .iter()
        .any(|i| i.why == "waiting"));
    // a factor under 1 would call every step slow: it is taken as 1
    assert!(tower::timeline(&steps, 2.0, 0.1)
        .iter()
        .all(|i| i.light == Light::Green));
}

#[test]
fn the_tower_report_names_what_is_not_green() {
    let mut steps = job();
    steps[1].result = "PHOENIX-PNP-023".into();
    let t = tower::timeline(&steps, 2.0, 3.0);
    let text = tower::text(&t);
    assert!(text.contains("1.000 s to 2.000 s: red (failed)"));
    assert!(text.contains("6.000 s to 11.000 s: yellow (waiting)"));
    assert!(text.lines().last().unwrap().starts_with("green "));
    let csv = tower::csv(&t);
    assert!(csv.starts_with("from_s,to_s,light,why\n"));
    assert!(csv.contains("1.000,2.000,red,failed"));
    assert_eq!(Light::Red.name(), "red");
}

#[test]
fn journals_made_on_a_fixed_seed_give_a_tower_that_covers_the_job_without_a_hole() {
    let mut x: u64 = 0x70E617;
    let mut next = || {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (x >> 33) as f64 / (1u64 << 31) as f64
    };
    for round in 0..300 {
        let mut at = 0.0;
        let steps: Vec<Step> = (0..1 + (next() * 30.0) as usize)
            .map(|i| {
                let took = 0.1 + next() * 3.0;
                let start = at + if next() < 0.2 { next() * 6.0 } else { 0.0 };
                at = start + took;
                let result = if next() < 0.05 {
                    "PHOENIX-PNP-022"
                } else {
                    "ok"
                };
                step(
                    i + 1,
                    start,
                    took,
                    if i % 2 == 0 { "pick" } else { "place" },
                    "F1",
                    1 + i % 2,
                    result,
                )
            })
            .collect();
        let t = tower::timeline(&steps, 1.0, 2.0);
        let total: f64 = tower::seconds(&t).iter().sum();
        let span = steps.last().map(|s| s.at_s + s.took_s).unwrap() - steps[0].at_s;
        assert!((total - span).abs() < 1e-6, "round {round}: {total} {span}");
        for pair in t.windows(2) {
            assert!(
                (pair[0].to_s - pair[1].from_s).abs() < 1e-6 || pair[1].from_s > pair[0].to_s,
                "round {round}"
            );
        }
        let b = analyse(&steps, 3);
        assert!((b.total_s - span).abs() < 1e-6);
        assert!(b.nozzles.iter().all(|u| u.share <= 1.0 + 1e-9));
    }
}

#[test]
fn the_settings_of_the_views_are_keys_with_a_range() {
    for k in ["tower_idle_gap_s", "tower_slow_factor", "busy_gaps_listed"] {
        assert!(KEYS.contains(&k), "{k}");
    }
    let d = MachineConfig::default();
    assert_eq!(
        (d.tower_idle_gap_s, d.tower_slow_factor, d.busy_gaps_listed),
        (2.0, 3.0, 5)
    );
    let c = MachineConfig::parse("tower_slow_factor = 5\nbusy_gaps_listed = 1\n").unwrap();
    assert_eq!((c.tower_slow_factor, c.busy_gaps_listed), (5.0, 1));
    for bad in [
        "tower_slow_factor = 0.5",
        "busy_gaps_listed = 0",
        "tower_idle_gap_s = -1",
    ] {
        let e = MachineConfig::parse(bad).unwrap_err();
        assert_eq!(e.code, Code::ConfigOutOfRange, "{bad}");
    }
}
