//! The picks of a job feeder by feeder: the rates of failure, the feeders that are marked, the plan's share of each feeder and the CSV and text of the result.

use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::feedrep::{csv, rows_from, text, CSV_HEADER};
use eh3d_phoenix_pnp::job::FeederStats;
use eh3d_phoenix_pnp::planner::PickTime;

const MACHINE: &str = include_str!("../examples/machine.cfg");

fn stats(id: &str, picked: u32, pick_failures: u32, faults: u32, disabled: bool) -> FeederStats {
    FeederStats {
        id: id.into(),
        picked,
        faults,
        disabled,
        pick_failures,
    }
}

fn pick(feeder: &str, busy_s: f64) -> PickTime {
    PickTime {
        feeder: feeder.into(),
        at_s: 0.0,
        busy_s,
    }
}

#[test]
fn the_rate_of_failure_counts_the_picks_that_worked_and_the_ones_that_failed() {
    let rows = rows_from(
        &[stats("F1", 90, 10, 0, false), stats("F2", 0, 0, 0, false)],
        &[],
        20.0,
        10,
    );
    assert!((rows[0].fail_percent - 10.0).abs() < 1e-9);
    assert_eq!(rows[1].fail_percent, 0.0, "nothing tried is no failure");
    assert!(!rows[0].flagged && !rows[1].flagged);
}

#[test]
fn a_feeder_is_marked_only_above_the_limit_and_once_it_was_tried_enough_times() {
    let s = [
        stats("A", 6, 4, 0, false),
        stats("B", 7, 3, 0, false),
        stats("C", 1, 3, 0, false),
        stats("D", 70, 30, 0, false),
    ];
    // 40, 30, 75 and 30 per cent; the limit is 30, the least number of picks 10
    let rows = rows_from(&s, &[], 30.0, 10);
    let flagged: Vec<&str> = rows
        .iter()
        .filter(|r| r.flagged)
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(
        flagged,
        ["A"],
        "30 per cent is on the limit, 4 picks are too few to say"
    );
    let rows = rows_from(&s, &[], 30.0, 4);
    let flagged: Vec<&str> = rows
        .iter()
        .filter(|r| r.flagged)
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(flagged, ["A", "C"]);
    // a limit of 100 never marks a feeder
    assert!(rows_from(&s, &[], 100.0, 1).iter().all(|r| !r.flagged));
}

#[test]
fn the_plan_gives_each_feeder_its_picks_and_its_busy_seconds() {
    let picks = [
        pick("F1", 0.5),
        pick("F2", 0.25),
        pick("F1", 0.5),
        pick("GONE", 9.0),
    ];
    let rows = rows_from(
        &[
            stats("F1", 2, 0, 0, false),
            stats("F2", 1, 0, 0, false),
            stats("F3", 0, 0, 0, false),
        ],
        &picks,
        20.0,
        10,
    );
    assert_eq!((rows[0].planned_picks, rows[0].planned_busy_s), (2, 1.0));
    assert_eq!((rows[1].planned_picks, rows[1].planned_busy_s), (1, 0.25));
    assert_eq!((rows[2].planned_picks, rows[2].planned_busy_s), (0, 0.0));
}

#[test]
fn the_csv_keeps_its_header_and_the_order_of_the_feeders() {
    let rows = rows_from(
        &[stats("F1", 8, 2, 1, false), stats("F,2", 5, 0, 0, true)],
        &[pick("F1", 0.5)],
        10.0,
        5,
    );
    let out = csv(&rows);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        CSV_HEADER,
        "feeder,planned_picks,planned_busy_s,picked,pick_failures,fail_percent,faults,disabled,flagged"
    );
    assert_eq!(lines[0], CSV_HEADER);
    assert_eq!(lines[1], "F1,1,0.500,8,2,20.0,1,no,yes");
    assert_eq!(lines[2], "\"F,2\",0,0.000,5,0,0.0,0,yes,no");
    assert_eq!(lines.len(), 3);
}

#[test]
fn the_text_puts_the_feeders_that_fail_most_first_and_says_which_to_look_at() {
    let rows = rows_from(
        &[
            stats("F1", 99, 1, 0, false),
            stats("F2", 60, 40, 3, true),
            stats("F3", 90, 10, 0, false),
        ],
        &[pick("F2", 1.5)],
        30.0,
        10,
    );
    let t = text(&rows);
    let lines: Vec<&str> = t.lines().collect();
    assert_eq!(lines.len(), 3);
    assert!(
        lines[0].starts_with(
            "feeder F2: 60 picked, 40 failed (40.0 per cent), 3 faults, plan 1 picks in 1.5 s"
        ),
        "{t}"
    );
    assert!(
        lines[0].ends_with(", switched off <- look at this feeder"),
        "{t}"
    );
    assert!(lines[1].starts_with("feeder F3:") && !lines[1].contains("look at"));
    assert!(lines[2].starts_with("feeder F1:"));
}

#[test]
fn the_settings_of_the_report_are_in_the_example_with_their_defaults_and_ranges() {
    for k in ["feeder_fail_percent", "feeder_fail_min_picks"] {
        assert!(KEYS.contains(&k), "{k}");
        assert!(MACHINE.contains(&format!("\n{k} = ")), "{k}");
    }
    let e = MachineConfig::parse(MACHINE).unwrap();
    let d = MachineConfig::default();
    assert_eq!((e.feeder_fail_percent, e.feeder_fail_min_picks), (20.0, 10));
    assert_eq!((d.feeder_fail_percent, d.feeder_fail_min_picks), (20.0, 10));
    for (key, ok, bad) in [
        ("feeder_fail_percent", ["0", "100"], ["-1", "100.1"]),
        ("feeder_fail_min_picks", ["1", "100000"], ["0", "100001"]),
    ] {
        for v in ok {
            assert!(
                MachineConfig::parse(&format!("{key} = {v}")).is_ok(),
                "{key} {v}"
            );
        }
        for v in bad {
            assert_eq!(
                MachineConfig::parse(&format!("{key} = {v}"))
                    .unwrap_err()
                    .code,
                Code::ConfigOutOfRange,
                "{key} {v}"
            );
        }
    }
}
