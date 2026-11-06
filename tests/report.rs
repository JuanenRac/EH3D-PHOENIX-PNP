//! The report of a job: where the time went, the picks at the first attempt, the correction of the camera in histograms, the groups, the errors with their codes, what the person decided
//! when an error stopped the job, and the text, the CSV, the JSON and the page that show it.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::{Code, Lang};
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::job::{Action, Job, Phase, Status, STATS_CSV_HEADER};
use eh3d_phoenix_pnp::model::{Machine, Nozzle};
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use eh3d_phoenix_pnp::report::{
    self, build, histogram, write_new, ReportCheck, ReportInput, HIST_BINS,
};
use eh3d_phoenix_pnp::sim::SimOp;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nR3,10k,R,30,10,180,top\nC1,100n,C,10,20,0,top\nC2,100n,C,20,20,0,top\nU1,MCU,Q,40,15,0,top\n";
const FEEDERS: &str =
    "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\n";
const TWO_OF_EACH: &str = "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF1b,10k,R,-20,15,10\nF2,100n,C,-20,20,10\nF2b,100n,C,-20,25,10\nF3,MCU,Q,-20,40,2\nF3b,MCU,Q,-20,45,2\n";
const OFFSET: &str =
    "align = on\nsim_align_dx_mm = 0.2\nsim_align_dy_mm = -0.1\nsim_align_deg = 3\n";
const STOPS: &str = "error_policy = alert\npick_retries = 0\n";

fn job_with(parts: &str, feeders: &str, cfg: &str) -> Job {
    let cfg = MachineConfig::parse(cfg).unwrap();
    let parts = from_csv(parts).unwrap();
    let feeders = feeding::from_csv(feeders).unwrap();
    let picks = assign(&parts, &feeders, Some(Side::Top));
    let plan = plan_advanced(
        &parts,
        &feeders,
        &picks,
        &Catalog::new(),
        &cfg,
        &Options::default(),
    )
    .unwrap();
    Job::new(plan, feeders, cfg).unwrap()
}

fn job(cfg: &str) -> Job {
    job_with(PARTS, FEEDERS, cfg)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn none() -> ReportInput {
    ReportInput::default()
}

/// A job whose first pick fails and stops it.
fn stopped(feeders: &str) -> Job {
    let mut j = job_with(PARTS, feeders, STOPS);
    j.machine_mut().force_fail(SimOp::Pick, 1);
    let e = j.run(None).unwrap_err();
    assert_eq!(e.code, Code::PickFailed);
    assert_eq!(j.phase(), Phase::Stopped);
    j
}

// ---- the time

#[test]
fn the_phases_add_up_to_the_clock_of_the_machine_whatever_happens() {
    let failing = "sim_pick_fail_rate = 0.3\nsim_feed_fail_rate = 0.2\nsim_place_fail_rate = 0.2\nsim_seed = 11\nerror_policy = defer\npick_retries = 1\nplace_retries = 1\nfeed_retries = 1";
    let all_lost =
        "error_policy = defer\nsim_pick_fail_rate = 1\nfault_limit = 0\npick_retries = 1\n";
    let camera = format!("{OFFSET}align_passes = 2\nalign_settle_s = 0.25\nsim_camera_fail_rate = 0.3\nerror_policy = defer\nsim_seed = 5\n");
    for cfg in ["", failing, all_lost, OFFSET, camera.as_str()] {
        let mut j = job(cfg);
        j.run(None).unwrap();
        assert_eq!(
            j.trace().total_us(),
            j.machine().now_us(),
            "the phases of {cfg:?} do not add up"
        );
    }
    // also a job that an error stopped
    let j = stopped(FEEDERS);
    assert_eq!(j.trace().total_us(), j.machine().now_us());
}

#[test]
fn a_plain_job_spends_the_time_of_its_picks_its_places_and_its_moves() {
    let mut j = job("");
    j.run(None).unwrap();
    let t = j.trace();
    // six parts: 0.2 s of feed and 0.4 s of pick each, and 0.4 s of place each
    assert_eq!(t.pick_us, 3_600_000);
    assert_eq!(t.place_us, 2_400_000);
    assert_eq!((t.align_us, t.tip_us), (0, 0));
    assert_eq!(t.travel_us, j.machine().now_us() - 6_000_000);
    assert!(t.travel_us > 0);
    let r = build(&j, &none());
    assert!(near(r.seconds_pick, 3.6) && near(r.seconds_place, 2.4));
    assert!(near(r.seconds_total, j.machine().now_us() as f64 / 1e6));
    assert!(near(r.seconds_per_part, r.seconds_total / 6.0));
}

#[test]
fn the_waits_and_the_looks_of_the_camera_are_a_phase_of_their_own() {
    let mut j = job(OFFSET);
    j.run(None).unwrap();
    // one look of 0.1 s of settling for each of the six parts
    assert_eq!(j.trace().align_us, 600_000);
    assert_eq!(j.trace().pick_us, 3_600_000);
    let mut three = job(&format!(
        "{OFFSET}align_passes = 3\nalign_settle_s = 0.25\n"
    ));
    three.run(None).unwrap();
    assert_eq!(three.trace().align_us, 6 * 3 * 250_000);
}

// ---- the picks

#[test]
fn a_pick_that_needs_a_second_attempt_is_not_a_pick_at_the_first() {
    let mut j = job("");
    j.machine_mut().force_fail(SimOp::Pick, 1);
    j.run(None).unwrap();
    let t = j.trace();
    assert_eq!((t.first_try, t.retried, t.lost), (5, 1, 0));
    let r = build(&j, &none());
    assert!(near(r.first_try_ratio(), 5.0 / 6.0));
    // the one error that was raised is the fault that was made
    assert_eq!(r.codes, vec![(Code::SimFault, 1)]);
}

#[test]
fn parts_that_are_never_picked_are_counted_with_the_errors_that_came() {
    let mut j =
        job("error_policy = defer\nsim_pick_fail_rate = 1\nfault_limit = 0\npick_retries = 1\n");
    j.run(None).unwrap();
    let t = j.trace();
    assert_eq!((t.first_try, t.retried, t.lost), (0, 0, 6));
    let r = build(&j, &none());
    assert!(near(r.first_try_ratio(), 0.0));
    // two attempts for each of the six parts, and each part given up
    assert_eq!(r.codes, vec![(Code::PickFailed, 6), (Code::SimFault, 12)]);
    assert_eq!((r.placed, r.deferred, r.skipped, r.pending), (0, 6, 0, 0));
    assert_eq!(r.put_aside.len(), 6);
}

#[test]
fn nothing_was_tried_so_the_ratio_is_zero() {
    let j = job("");
    assert!(near(build(&j, &none()).first_try_ratio(), 0.0));
}

// ---- the correction of the camera

#[test]
fn the_histograms_hold_the_first_reading_of_every_part() {
    let mut j = job(OFFSET);
    j.run(None).unwrap();
    let r = build(&j, &none());
    assert_eq!(r.histograms.len(), 3);
    let (dx, dy, turn) = (&r.histograms[0], &r.histograms[1], &r.histograms[2]);
    assert_eq!((dx.name, dy.name, turn.name), ("dx", "dy", "turn"));
    // the tolerance is 0.5 mm, so the bins are 0.125 mm wide from -0.5: 0.2 is in the sixth (0.7 / 0.125 = 5.6) and -0.1 in the fourth (0.4 / 0.125 = 3.2)
    assert!(near(dx.lo, -0.5) && near(dx.hi, 0.5));
    assert_eq!(dx.counts, vec![0, 0, 0, 0, 0, 6, 0, 0]);
    assert_eq!(dy.counts, vec![0, 0, 0, 6, 0, 0, 0, 0]);
    // the largest turn is 3 degrees, so the range is -3 to 3 and a turn of 3 is in the last bin
    assert!(near(turn.lo, -3.0) && near(turn.hi, 3.0));
    assert_eq!(turn.counts, vec![0, 0, 0, 0, 0, 0, 0, 6]);
    assert!(near(dx.mean, 0.2) && near(dy.mean, -0.1) && near(turn.mean, 3.0));
    assert!(near(dx.bin(5).0, 0.125) && near(dx.bin(5).1, 0.25));
    for h in &r.histograms {
        assert_eq!(h.counts.len(), HIST_BINS);
        assert_eq!(h.counts.iter().sum::<u32>(), 6);
    }
    assert_eq!(r.corrected, 6);
}

#[test]
fn a_part_thrown_away_by_the_tolerance_is_not_in_the_histograms() {
    let mut j = job("align = on\nsim_align_dx_mm = 0.6\npick_tolerance_mm = 0.5\nfault_limit = 0\nerror_policy = defer\npick_retries = 1\n");
    j.run(None).unwrap();
    let r = build(&j, &none());
    assert_eq!((r.corrected, r.over_tolerance, r.no_reading), (0, 12, 0));
    assert!(r
        .histograms
        .iter()
        .all(|h| h.counts.iter().sum::<u32>() == 0));
    // each look over the tolerance raised the code, and each part given up raised it again
    assert!(r.codes.contains(&(Code::AlignRejected, 12 + 6)));
}

#[test]
fn without_the_align_step_the_report_has_no_camera() {
    let mut j = job("");
    j.run(None).unwrap();
    let r = build(&j, &none());
    assert!(!r.align_on && r.histograms.is_empty());
    assert!(!r.text(Lang::En).contains("Camera"));
    assert!(!r.csv_sections().contains("histogram,"));
    assert!(r.json().get("camera").is_none());
}

#[test]
fn the_histogram_puts_what_is_outside_in_the_bins_at_the_ends_and_ignores_what_is_not_a_number() {
    let values = [
        -1.0,
        0.0,
        0.99,
        1.0,
        3.9,
        4.0,
        100.0,
        f64::NAN,
        f64::INFINITY,
    ];
    assert_eq!(histogram(&values, 0.0, 4.0, 4), vec![3, 1, 0, 3]);
    assert_eq!(histogram(&values, 4.0, 0.0, 4), vec![0, 0, 0, 0]);
    assert_eq!(histogram(&values, 0.0, 4.0, 0), Vec::<u32>::new());
}

// ---- the groups

#[test]
fn the_parts_are_grouped_by_nozzle_by_tip_and_by_board_of_the_panel() {
    let panel = "ref,val,package,x,y,rot,side\nA/R1,10k,R,10,10,0,top\nA/R2,10k,R,20,10,0,top\nA/C1,100n,C,10,20,0,top\nB/R1,10k,R,60,10,0,top\nB/R2,10k,R,70,10,0,top\nB/C1,100n,C,60,20,0,top\n";
    let mut j = job_with(panel, FEEDERS, "nozzles = 2\n");
    j.run(None).unwrap();
    let r = build(&j, &none());
    let boards: Vec<(&str, usize, usize)> = r
        .boards
        .iter()
        .map(|g| (g.name.as_str(), g.planned, g.placed))
        .collect();
    assert_eq!(boards.len(), 2);
    assert!(boards.contains(&("A", 3, 3)) && boards.contains(&("B", 3, 3)));
    assert_eq!(r.nozzles.iter().map(|g| g.planned).sum::<usize>(), 6);
    assert_eq!(r.tips.iter().map(|g| g.planned).sum::<usize>(), 6);
    assert!(r.nozzles.iter().all(|g| g.not_placed() == 0));
    // the nozzles come in order
    let names: Vec<&str> = r.nozzles.iter().map(|g| g.name.as_str()).collect();
    assert!(names == ["1", "2"] || names == ["1"], "{names:?}");
    // a part of a single board is in the group without a name
    let mut single = job("");
    single.run(None).unwrap();
    assert_eq!(build(&single, &none()).boards[0].name, "-");
}

// ---- what the person decides

#[test]
fn retry_goes_on_with_the_step_that_failed() {
    let mut j = stopped(FEEDERS);
    let f = j.failure().unwrap();
    assert!(f.at_pick);
    assert_eq!(f.error.code, Code::PickFailed);
    assert_eq!(f.reference, j.plan().plan.placements[f.index].reference);
    j.act(Action::Retry).unwrap();
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    let r = build(&j, &none());
    assert_eq!((r.placed, r.skipped, r.deferred), (6, 0, 0));
    assert_eq!(r.actions, [1, 0, 0, 0]);
    // the part picked again is not a pick at the first attempt: five were, one needed the retry
    assert_eq!((r.first_try, r.retried, r.lost), (5, 1, 0));
}

#[test]
fn skip_leaves_the_part_out_and_the_job_goes_on() {
    let mut j = stopped(FEEDERS);
    let f = j.failure().unwrap();
    j.act(Action::Skip).unwrap();
    assert_eq!(j.status()[f.index], Status::Skipped);
    assert_eq!(j.phase(), Phase::Ready);
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    assert_eq!((j.stats().placed, j.stats().skipped), (5, 1));
    assert_eq!(j.stats().deferred, 0);
    let r = build(&j, &none());
    assert_eq!((r.placed, r.skipped, r.pending), (5, 1, 0));
    assert_eq!(r.left_out.len(), 1);
    assert_eq!(r.left_out[0].0, f.reference);
    assert!(
        r.left_out[0]
            .1
            .starts_with("by the person, after: PHOENIX-PNP-022"),
        "{}",
        r.left_out[0].1
    );
    assert_eq!(r.actions, [0, 1, 0, 0]);
    assert_eq!((r.first_try, r.retried, r.lost), (5, 0, 1));
    // the errors: the fault that was made and the pick that stopped the job
    assert_eq!(r.codes, vec![(Code::PickFailed, 1), (Code::SimFault, 1)]);
    let s: Value = serde_json::from_str(&j.stats_json()).unwrap();
    assert_eq!(s["skipped"], 1);
    assert_eq!(s["placed"], 5);
    assert_eq!(j.machine().holding(0), None);
}

#[test]
fn defer_puts_the_part_aside_with_the_reason_of_the_person() {
    let mut j = stopped(FEEDERS);
    let f = j.failure().unwrap();
    j.act(Action::Defer).unwrap();
    assert_eq!(j.status()[f.index], Status::Deferred);
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    assert_eq!((j.stats().placed, j.stats().deferred), (5, 1));
    let aside = j.deferred();
    assert_eq!(aside.len(), 1);
    assert_eq!(aside[0].0, f.reference);
    assert!(aside[0]
        .1
        .starts_with("by the person, after: PHOENIX-PNP-022"));
    let r = build(&j, &none());
    assert_eq!(r.actions, [0, 0, 1, 0]);
    assert_eq!(r.put_aside, aside);
}

#[test]
fn a_part_that_fails_in_the_place_can_be_left_out_with_the_part_dropped() {
    let mut j = job_with(PARTS, FEEDERS, "error_policy = alert\nplace_retries = 0\n");
    j.machine_mut().force_fail(SimOp::Place, 1);
    let e = j.run(None).unwrap_err();
    assert_eq!(e.code, Code::PlaceFailed);
    let f = j.failure().unwrap();
    assert!(!f.at_pick);
    assert_eq!(j.machine().holding(f.nozzle), Some(f.reference.as_str()));
    j.act(Action::Skip).unwrap();
    // the part that could not be placed is not left on the nozzle for the next pick
    assert_eq!(j.machine().holding(f.nozzle), None);
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    assert_eq!((j.stats().placed, j.stats().skipped), (5, 1));
    // it was picked, so it is not a part that was never picked
    assert_eq!(j.trace().lost, 0);
}

#[test]
fn switching_off_the_feeder_hands_the_parts_to_another_feeder_of_the_kind() {
    let mut j = stopped(TWO_OF_EACH);
    let f = j.failure().unwrap();
    j.act(Action::SwitchOffFeeder).unwrap();
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    let r = build(&j, &none());
    assert_eq!((r.placed, r.skipped, r.deferred), (6, 0, 0));
    let off = r.feeders.iter().find(|x| x.id == f.feeder).unwrap();
    assert!(off.disabled);
    // the feeder failed at the first pick of the job, so it never gave a part
    assert_eq!(off.picked, 0);
    assert_eq!(r.feeders.iter().map(|x| x.picked).sum::<u32>(), 6);
    assert_eq!(r.actions, [0, 0, 0, 1]);
    assert!(j
        .machine()
        .events
        .iter()
        .any(|e| e.text == format!("feeder {} switched off by the person", f.feeder)));
}

#[test]
fn an_action_that_does_not_fit_the_job_is_refused_and_changes_nothing() {
    let mut running = job("");
    for a in Action::ALL {
        let e = running.act(a).unwrap_err();
        assert_eq!(e.code, Code::JobActionBad);
    }
    assert_eq!(running.phase(), Phase::Ready);
    assert_eq!(running.stats().skipped, 0);
    assert_eq!(running.trace().actions, [0; 4]);
    // once a feeder is off, the job runs again and cannot be asked to switch it off a second time
    let mut j = stopped(TWO_OF_EACH);
    j.act(Action::SwitchOffFeeder).unwrap();
    assert_eq!(
        j.act(Action::SwitchOffFeeder).unwrap_err().code,
        Code::JobActionBad
    );
    // a finished job has nothing to decide either
    j.run(None).unwrap();
    assert_eq!(j.act(Action::Retry).unwrap_err().code, Code::JobActionBad);
    assert!(j.failure().is_none());
}

#[test]
fn without_another_feeder_switching_the_feeder_off_stops_the_job_again() {
    let mut j = stopped(FEEDERS);
    j.act(Action::SwitchOffFeeder).unwrap();
    let e = j.run(None).unwrap_err();
    assert_eq!(e.code, Code::FeederUnavailable);
    // the person can now leave that part out
    j.act(Action::Skip).unwrap();
    assert_eq!(j.phase(), Phase::Ready);
}

// ---- a saved job keeps its report

#[test]
fn a_saved_job_resumed_anywhere_gives_the_report_of_the_job_that_never_stopped() {
    let cfg = "sim_pick_fail_rate = 0.3\nsim_feed_fail_rate = 0.2\nsim_place_fail_rate = 0.2\nsim_seed = 11\nerror_policy = defer\npick_retries = 1\nplace_retries = 1\nfeed_retries = 1\nalign = on\nsim_align_dx_mm = 0.2\nsim_align_dy_mm = -0.1\nsim_align_deg = 3\nsim_align_noise_mm = 0.05\n";
    let mut straight = job(cfg);
    straight.run(None).unwrap();
    let want = build(&straight, &none());
    for stop_at in [0usize, 1, 5, 9] {
        let mut first = job(cfg);
        first.run(Some(stop_at)).unwrap();
        let saved = first.save_state();
        let mut second = job(cfg);
        second.restore_state(&saved).unwrap();
        second.resume();
        second.run(None).unwrap();
        assert_eq!(
            build(&second, &none()),
            want,
            "stopped after {stop_at} steps"
        );
        assert_eq!(second.save_state(), straight.save_state());
    }
}

#[test]
fn a_state_with_the_decisions_of_the_person_comes_back_with_them() {
    let mut j = stopped(FEEDERS);
    j.act(Action::Skip).unwrap();
    j.run(Some(2)).unwrap();
    let saved = j.save_state();
    let mut again = job_with(PARTS, FEEDERS, STOPS);
    again.restore_state(&saved).unwrap();
    assert_eq!(again.stats().skipped, 1);
    assert!(again.status().contains(&Status::Skipped));
    assert_eq!(build(&again, &none()).actions, [0, 1, 0, 0]);
    assert_eq!(again.trace(), j.trace());
    // a damaged trace is refused and changes nothing
    let damaged = saved.replace("\"first_try\"", "\"first_tri\"");
    let mut third = job_with(PARTS, FEEDERS, STOPS);
    assert_eq!(
        third.restore_state(&damaged).unwrap_err().code,
        Code::JobStateBad
    );
    assert_eq!(third.stats().skipped, 0);
    // a state saved before the trace existed starts it again from zero
    let mut old: Value = serde_json::from_str(&saved).unwrap();
    old.as_object_mut().unwrap().remove("trace");
    let mut fourth = job_with(PARTS, FEEDERS, STOPS);
    fourth.restore_state(&old.to_string()).unwrap();
    assert_eq!(fourth.trace().total_us(), 0);
}

// ---- nothing changes for a job that nobody touches

#[test]
fn the_statistics_of_a_plain_job_are_what_they_always_were() {
    let mut j = job("");
    j.run(None).unwrap();
    let json = j.stats_json();
    assert!(!json.contains("skipped") && !json.contains("work"));
    let csv = j.stats_csv();
    assert!(!csv.contains("work,") && !csv.contains("time,"));
    assert_eq!(csv.lines().count(), 1 + 10 + 3 + 3);
}

// ---- the text, the CSV, the JSON and the page

#[test]
fn the_text_is_in_the_eight_languages() {
    assert!(report::words_complete());
    let mut j = job(OFFSET);
    j.run(None).unwrap();
    let r = build(&j, &none());
    let titles = [
        (Lang::En, "Work report"),
        (Lang::Es, "Informe de trabajo"),
        (Lang::De, "Arbeitsbericht"),
        (Lang::Fr, "Rapport de travail"),
        (Lang::It, "Rapporto di lavoro"),
        (Lang::Ja, "作業レポート"),
        (Lang::Zh, "作业报告"),
        (Lang::Pt, "Relatório de trabalho"),
    ];
    let mut seen: Vec<String> = Vec::new();
    for (lang, title) in titles {
        let text = r.text(lang);
        assert!(
            text.starts_with(title),
            "{lang:?}: {}",
            text.chars().take(40).collect::<String>()
        );
        assert!(
            !seen.contains(&text),
            "{lang:?} has the text of another language"
        );
        seen.push(text);
    }
    // the codes keep their number and take their meaning from the language
    let mut bad =
        job("error_policy = defer\nsim_pick_fail_rate = 1\nfault_limit = 0\npick_retries = 0\n");
    bad.run(None).unwrap();
    let r = build(&bad, &none());
    assert!(r.text(Lang::En).contains("PHOENIX-PNP-031"));
    assert!(r.text(Lang::Es).contains(&format!(
        "PHOENIX-PNP-022  {}",
        Code::PickFailed.message(Lang::Es)
    )));
}

#[test]
fn the_text_names_the_findings_the_parts_not_mounted_and_the_parts_put_aside() {
    let mut j =
        job("error_policy = defer\nsim_pick_fail_rate = 1\nfault_limit = 0\npick_retries = 0\n");
    j.run(None).unwrap();
    let input = ReportInput {
        dnp: vec!["R9".into(), "C9".into()],
        checks: vec![ReportCheck {
            severity: "warning".into(),
            source: "F1".into(),
            message: "few parts left".into(),
        }],
    };
    let text = build(&j, &input).text(Lang::En);
    assert!(text.contains("Not to be mounted (DNP): 2"), "{text}");
    assert!(text.contains("R9, C9"), "{text}");
    assert!(text.contains("WARNING F1: few parts left"), "{text}");
    assert!(text.contains("Parts put aside"), "{text}");
    assert!(text.contains("Put aside: 6"), "{text}");
    assert!(text.contains("PHOENIX-PNP-022"), "{text}");
}

#[test]
fn the_csv_keeps_its_header_and_its_rows_and_adds_sections_after_them() {
    let mut j = job(OFFSET);
    j.run(None).unwrap();
    let old = j.stats_csv();
    let input = ReportInput {
        dnp: vec!["R9".into()],
        checks: Vec::new(),
    };
    let text = report::csv(&j, &input);
    // the rows that were always there come first, the sections of the report follow, and the section of the align step stays the last
    let split = old
        .find(
            "
align,",
        )
        .unwrap()
        + 1;
    assert!(text.starts_with(&old[..split]));
    assert!(text.ends_with(&old[split..]));
    let mut plain = job("");
    plain.run(None).unwrap();
    assert!(report::csv(&plain, &none()).starts_with(&plain.stats_csv()));
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], STATS_CSV_HEADER);
    for l in &lines {
        assert_eq!(l.split(',').count(), 9, "{l}");
    }
    for expected in [
        "work,planned,6,,,,,,",
        "work,first_try,6,,,,,,",
        "work,first_try_ratio,1.000,,,,,,",
        "work,dnp,1,,,,,,",
        "time,pick,3.600,,,,,,",
        "time,place,2.400,,,,,,",
        "time,camera,0.600,,,,,,",
        "histogram,dx_mm:0.125..0.250,6,,,,,,",
        "histogram,dy_mm:-0.125..0.000,6,,,,,,",
        "histogram,turn_deg:2.250..3.000,6,,,,,,",
        "nozzle,1,6,,,6,,0,",
        "dnp,R9,,,,,,,",
    ] {
        assert!(lines.contains(&expected), "{expected} is not in\n{text}");
    }
    assert_eq!(
        lines.iter().filter(|l| l.starts_with("histogram,")).count(),
        3 * HIST_BINS
    );
}

#[test]
fn the_json_keeps_its_keys_and_adds_the_work() {
    let mut j = job(OFFSET);
    j.run(None).unwrap();
    let doc: Value = serde_json::from_str(&report::json(&j, &none())).unwrap();
    let old: Value = serde_json::from_str(&j.stats_json()).unwrap();
    for (k, v) in old.as_object().unwrap() {
        assert_eq!(&doc[k], v, "{k}");
    }
    let w = &doc["work"];
    assert_eq!(w["state"], "finished");
    assert_eq!(w["parts"]["placed"], 6);
    assert_eq!(w["first_try"]["picked_first"], 6);
    assert!(near(w["seconds"]["pick"].as_f64().unwrap(), 3.6));
    assert!(near(w["seconds"]["camera"].as_f64().unwrap(), 0.6));
    assert_eq!(w["histograms"]["dx"]["counts"][5], 6);
    assert_eq!(w["camera"]["corrected"], 6);
    assert_eq!(w["nozzles"][0]["planned"], 6);
}

#[test]
fn the_page_holds_everything_and_escapes_every_text() {
    let mut j = job(OFFSET);
    j.run(None).unwrap();
    let input = ReportInput {
        dnp: vec!["<script>alert(1)</script>".into()],
        checks: vec![ReportCheck {
            severity: "warning".into(),
            source: "\"F1\" & 'F2'".into(),
            message: "<img src=x onerror=alert(1)>".into(),
        }],
    };
    for lang in Lang::ALL {
        let page = build(&j, &input).html(lang);
        assert!(page.starts_with("<!DOCTYPE html>\n"));
        assert!(page.contains(&format!("<html lang=\"{}\">", lang.code())));
        assert!(page.contains("<meta charset=\"utf-8\">"));
        // nothing is run and nothing is fetched
        let lower = page.to_lowercase();
        for forbidden in [
            "<script",
            "<img",
            "<link",
            "<iframe",
            "<object",
            "<embed",
            "http:",
            "https:",
            "href=",
            "url(",
            "@import",
            "javascript:",
        ] {
            assert!(!lower.contains(forbidden), "{lang:?}: {forbidden}");
        }
        // every text of the input is escaped
        assert!(page.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(page.contains("&quot;F1&quot; &amp; &#39;F2&#39;"));
        assert!(page.contains("&lt;img src=x onerror=alert(1)&gt;"));
        // the tags are closed
        for tag in ["table", "tr", "ul", "head", "body", "html", "style"] {
            let open = page.matches(&format!("<{tag}")).count();
            let close = page.matches(&format!("</{tag}>")).count();
            assert_eq!(open, close, "{lang:?}: <{tag}>");
        }
    }
    assert_eq!(report::escape("a<b>&\"'"), "a&lt;b&gt;&amp;&quot;&#39;");
}

#[test]
fn a_page_is_never_written_over_a_file_that_exists() {
    let dir = scratch("write_new");
    let file = path(&dir, "report.html");
    write_new(&file, "first").unwrap();
    let e = write_new(&file, "second").unwrap_err();
    assert_eq!(e.code, Code::ReportExists);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "first");
    // a folder that does not exist is another error
    let e = write_new(&path(&dir, "no/such/report.html"), "x").unwrap_err();
    assert_ne!(e.code, Code::ReportExists);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the command

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
    let dir = std::env::temp_dir().join(format!("eh3d_pnp_report_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn path(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .output()
        .unwrap()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn simulate_writes_the_text_the_page_the_csv_and_the_json_of_the_report() {
    let dir = scratch("simulate");
    let (text, html, csv, json) = (
        path(&dir, "r.txt"),
        path(&dir, "r.html"),
        path(&dir, "r.csv"),
        path(&dir, "r.json"),
    );
    let o = run(&[
        "simulate",
        &example("board_dnp.csv"),
        &example("feeders.csv"),
        "--config",
        &example("machine.cfg"),
        "--packages",
        &example("packages.csv"),
        "--lang",
        "es",
        "--report-text",
        &text,
        "--report-html",
        &html,
        "--report-csv",
        &csv,
        "--report",
        &json,
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let t = std::fs::read_to_string(&text).unwrap();
    assert!(t.starts_with("Informe de trabajo"), "{t}");
    assert!(t.contains("No montar (DNP): 2"), "{t}");
    assert!(t.contains("R2, C2"), "{t}");
    let h = std::fs::read_to_string(&html).unwrap();
    assert!(h.contains("<html lang=\"es\">") && h.contains("Informe de trabajo"));
    let c = std::fs::read_to_string(&csv).unwrap();
    assert!(c.starts_with(STATS_CSV_HEADER));
    assert!(
        c.contains("dnp,R2,,,,,,,") && c.contains("dnp,C2,,,,,,,"),
        "{c}"
    );
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    assert_eq!(doc["work"]["parts"]["dnp"], 2);
    // the page is never written over; the text and the others are, as they always were
    let again = run(&[
        "simulate",
        &example("board_dnp.csv"),
        &example("feeders.csv"),
        "--report-html",
        &html,
    ]);
    assert_eq!(again.status.code(), Some(1));
    assert!(err(&again).contains("PHOENIX-PNP-040"), "{}", err(&again));
    assert_eq!(std::fs::read_to_string(&html).unwrap(), h);
    let again = run(&[
        "simulate",
        &example("board_dnp.csv"),
        &example("feeders.csv"),
        "--report-text",
        &text,
    ]);
    assert!(again.status.success(), "{}", err(&again));
    assert!(std::fs::read_to_string(&text)
        .unwrap()
        .starts_with("Work report"));
    // a report is never written over a file it was made from
    for option in ["--report-text", "--report-html"] {
        let same = run(&[
            "simulate",
            &example("board_dnp.csv"),
            &example("feeders.csv"),
            option,
            &example("feeders.csv"),
        ]);
        assert_eq!(same.status.code(), Some(1), "{option}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
