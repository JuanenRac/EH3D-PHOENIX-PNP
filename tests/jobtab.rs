//! The Job tab of the window without a screen: the state of every placement, the filters, the progress, the dialog of the error and its actions, the steps made for a picture and the
//! options that choose the tab of a picture.

use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::gui::app::{AppState, Tab};
use eh3d_phoenix_pnp::gui::jobtab::{
    action_text, counts, current_index, error_dialog, filter_rows, job_rows, place_state, progress,
    sort_job, JobFilter, JobRow, PlaceState,
};
use eh3d_phoenix_pnp::gui::samples;
use eh3d_phoenix_pnp::gui::settings::UiSettings;
use eh3d_phoenix_pnp::gui::shot::{parse_window_args, STEPS_MAX};
use eh3d_phoenix_pnp::gui::tables::{PlanColumn, SortState};
use eh3d_phoenix_pnp::gui::texts::{tr, Text};
use eh3d_phoenix_pnp::job::{Action, Phase};
use eh3d_phoenix_pnp::parts::Side;

const FAILS: &str =
    "error_policy = alert\npick_retries = 0\nsim_pick_fail_rate = 1\nfault_limit = 0\n";

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn app_of(parts_and_feeders: Vec<(String, String)>, cfg: &str) -> AppState {
    let mut a = AppState::new(UiSettings::default());
    let mut files = vec![("machine.cfg".to_string(), cfg.to_string())];
    files.extend(parts_and_feeders);
    assert_eq!(a.load_sample(&files), 3);
    assert!(a.planned.is_some());
    a
}

fn app(cfg: &str) -> AppState {
    app_of(samples::mini(), cfg)
}

fn rows_of(a: &AppState) -> Vec<JobRow> {
    job_rows(
        a.planned.as_ref().unwrap(),
        &a.ws.dnp,
        Side::Top,
        a.session.job(),
    )
}

/// Skips every part that stops the job until it finishes.
fn skip_all(a: &mut AppState) {
    for _ in 0..200 {
        if error_dialog(a.session.job()).is_some() {
            assert!(a.decide(Action::Skip));
        }
        a.tick(0.5);
        if a.session.phase() == Some(Phase::Finished) {
            return;
        }
    }
    panic!("the job did not finish");
}

// ---- the state of each placement

#[test]
fn a_placement_goes_from_waiting_to_picked_to_placed() {
    let mut a = app("");
    let rows = rows_of(&a);
    assert_eq!(rows.len(), 6);
    assert!(rows.iter().all(|r| r.state == PlaceState::Pending));
    assert_eq!(counts(&rows)[0], 6);
    a.step();
    // the nozzle holds the first part and the camera is off, so it is picked and not aligned
    let at = current_index(a.session.job()).unwrap();
    assert_eq!(place_state(a.session.job(), at), PlaceState::Picked);
    assert_eq!(rows_of(&a)[at].state, PlaceState::Picked);
    a.step();
    assert_eq!(place_state(a.session.job(), at), PlaceState::Placed);
    let counts = counts(&rows_of(&a));
    assert_eq!((counts[0], counts[3]), (5, 1));
    // no job: everything waits
    assert_eq!(place_state(None, 0), PlaceState::Pending);
}

#[test]
fn with_the_camera_on_a_picked_part_is_aligned_until_it_is_placed() {
    let mut a = app("align = on\nsim_align_dx_mm = 0.1\nsim_align_dy_mm = -0.05\n");
    a.step();
    let at = current_index(a.session.job()).unwrap();
    assert_eq!(place_state(a.session.job(), at), PlaceState::Aligned);
    a.step();
    assert_eq!(place_state(a.session.job(), at), PlaceState::Placed);
}

#[test]
fn the_step_that_an_error_stopped_is_failed_and_the_row_says_why() {
    let mut a = app(FAILS);
    a.step();
    assert_eq!(a.session.phase(), Some(Phase::Stopped));
    let rows = rows_of(&a);
    let failed: Vec<&JobRow> = rows
        .iter()
        .filter(|r| r.state == PlaceState::Failed)
        .collect();
    assert_eq!(failed.len(), 1);
    assert!(
        failed[0].note.starts_with("PHOENIX-PNP-022"),
        "{}",
        failed[0].note
    );
    // the failed row is the one of the step that is next
    assert_eq!(Some(failed[0].row.step - 1), current_index(a.session.job()));
}

#[test]
fn the_parts_not_to_be_mounted_are_rows_after_the_plan() {
    let parts = "ref,val,package,x,y,rot,side,dnp\nR1,10k,R_0603,10,10,0,top,no\nR2,10k,R_0603,20,10,90,top,yes\nC1,100n,C_0603,10,20,0,top,no\n";
    let mut files = samples::mini();
    files[0].1 = parts.to_string();
    let a = app_of(files, "");
    let rows = rows_of(&a);
    assert_eq!(rows.len(), 3);
    let last = &rows[2];
    assert_eq!(
        (last.row.reference.as_str(), last.state, last.row.step),
        ("R2", PlaceState::Dnp, 0)
    );
    assert!(rows[..2].iter().all(|r| r.state == PlaceState::Pending));
    // sorted by state, descending or not, they stay at the end
    for ascending in [true, false] {
        let mut sorted = rows.clone();
        sort_job(
            &mut sorted,
            SortState {
                column: PlanColumn::Status,
                ascending,
            },
        );
        assert_eq!(sorted[2].state, PlaceState::Dnp, "{ascending}");
    }
    // and the report knows them
    assert_eq!(a.report_input().dnp, vec!["R2".to_string()]);
}

#[test]
fn the_rows_sort_by_the_state_with_the_failed_first() {
    let mut a = app(FAILS);
    skip_all_but_one(&mut a);
    let mut rows = rows_of(&a);
    sort_job(&mut rows, SortState::new(PlanColumn::Status));
    // failed, then waiting, then left out
    let order: Vec<PlaceState> = rows.iter().map(|r| r.state).collect();
    assert_eq!(order[0], PlaceState::Failed);
    let skipped_at = order
        .iter()
        .position(|s| *s == PlaceState::Skipped)
        .unwrap();
    assert!(order[1..skipped_at]
        .iter()
        .all(|s| *s == PlaceState::Pending));
    assert!(order[skipped_at..]
        .iter()
        .all(|s| *s == PlaceState::Skipped));
    let mut by_reference = rows.clone();
    sort_job(
        &mut by_reference,
        SortState {
            column: PlanColumn::Reference,
            ascending: false,
        },
    );
    assert_eq!(by_reference[0].row.reference, "U1");
}

/// Skips two parts and leaves the job stopped at the third.
fn skip_all_but_one(a: &mut AppState) {
    a.step();
    for _ in 0..2 {
        assert!(a.decide(Action::Skip));
        for _ in 0..4 {
            a.tick(0.5);
        }
        assert_eq!(a.session.phase(), Some(Phase::Stopped));
    }
}

// ---- the filters

#[test]
fn the_filter_limits_the_rows_by_state_by_text_and_by_what_is_done() {
    let mut a = app(FAILS);
    a.step();
    skip_all(&mut a);
    let rows = rows_of(&a);
    assert_eq!(counts(&rows)[5], 6);
    let mut f = JobFilter::default();
    assert!(!f.is_active());
    assert_eq!(filter_rows(&rows, &f).len(), 6);
    f.state = Some(PlaceState::Skipped);
    assert_eq!(filter_rows(&rows, &f).len(), 6);
    f.state = Some(PlaceState::Placed);
    assert_eq!(filter_rows(&rows, &f).len(), 0);
    f.clear();
    f.text = "r1".to_string();
    assert!(f.is_active());
    let shown = filter_rows(&rows, &f);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].row.reference, "R1");
    // the value and the feeder count too, and the case does not
    f.text = " 10K ".to_string();
    assert_eq!(filter_rows(&rows, &f).len(), 3);
    f.text = "f3".to_string();
    assert_eq!(filter_rows(&rows, &f).len(), 1);
    f.clear();
    f.hide_done = true;
    assert!(f.is_active());
    assert_eq!(filter_rows(&rows, &f).len(), 0);
}

// ---- the progress

#[test]
fn the_progress_counts_the_parts_done_and_estimates_the_time_left() {
    let mut a = app("");
    let p = progress(a.session.job());
    assert_eq!((p.total, p.done), (0, 0));
    assert!(near(p.fraction, 0.0) && near(p.remaining_s, 0.0));
    a.step();
    a.step();
    let p = progress(a.session.job());
    assert_eq!((p.total, p.done, p.placed), (6, 1, 1));
    assert!(near(p.fraction, 1.0 / 6.0));
    assert!(p.elapsed_s > 0.0);
    // one part took all the time so far, so five more take five times as long
    assert!(near(p.remaining_s, p.elapsed_s * 5.0));
    a.run_steps(100);
    let p = progress(a.session.job());
    assert_eq!((p.done, p.placed), (6, 6));
    assert!(near(p.fraction, 1.0) && near(p.remaining_s, 0.0));
}

#[test]
fn before_the_first_part_is_done_the_time_left_is_the_time_of_the_plan() {
    let mut a = app("");
    a.step();
    let p = progress(a.session.job());
    assert_eq!(p.done, 0);
    assert!(near(
        p.remaining_s,
        a.planned.as_ref().unwrap().plan.plan.seconds
    ));
}

// ---- the dialog of the error and its actions

#[test]
fn the_dialog_names_the_failed_step_and_offers_the_four_actions() {
    let mut a = app(FAILS);
    assert!(error_dialog(a.session.job()).is_none());
    a.step();
    let d = error_dialog(a.session.job()).unwrap();
    assert_eq!(d.failure.error.code, Code::PickFailed);
    assert!(d.failure.at_pick);
    assert_eq!(d.failure.nozzle, 0);
    assert!(!d.failure.reference.is_empty() && !d.failure.feeder.is_empty());
    for action in Action::ALL {
        assert!(d.allows(action), "{action:?}");
    }
    // each action has a word in every language
    for action in Action::ALL {
        for lang in Lang::ALL {
            assert!(!tr(action_text(action), lang).is_empty());
        }
    }
    assert_eq!(tr(action_text(Action::Skip), Lang::Es), "Saltar la pieza");
}

#[test]
fn the_actions_are_carried_out_on_the_job_and_the_job_goes_on() {
    let mut a = app(FAILS);
    a.step();
    assert!(a.decide(Action::Skip));
    assert!(a.session.is_running());
    assert!(a.session.last_error().is_none());
    assert!(a
        .log
        .entries()
        .iter()
        .any(|e| e.text.contains("skip") && e.text.starts_with(tr(Text::ErrorTitle, Lang::En))));
    skip_all(&mut a);
    let job = a.session.job().unwrap();
    assert_eq!((job.stats().placed, job.stats().skipped), (0, 6));
    assert_eq!(a.session.progress(), (6, 6));
    let text = a.report_text().unwrap();
    assert!(text.starts_with("Work report"), "{text}");
    assert!(text.contains("Left out by the person: 6"), "{text}");
    assert!(text.contains("PHOENIX-PNP-022"), "{text}");
    // in the language of the window
    a.set_lang(Lang::Es);
    assert!(a.report_text().unwrap().starts_with("Informe de trabajo"));
}

#[test]
fn defer_puts_the_part_aside_and_switching_the_feeder_off_leaves_no_more_feeders() {
    let mut a = app(FAILS);
    a.step();
    assert!(a.decide(Action::Defer));
    a.tick(0.5);
    let job = a.session.job().unwrap();
    assert_eq!(job.stats().deferred, 1);
    assert_eq!(
        rows_of(&a)
            .iter()
            .filter(|r| r.state == PlaceState::Deferred)
            .count(),
        1
    );
    // the next part stops the job too; switch its feeder off
    for _ in 0..4 {
        a.tick(0.5);
    }
    assert_eq!(a.session.phase(), Some(Phase::Stopped));
    assert!(a.decide(Action::SwitchOffFeeder));
    for _ in 0..4 {
        a.tick(0.5);
    }
    // there is no other feeder of the kind: the job stops again, and that feeder can no longer be switched off
    let d = error_dialog(a.session.job()).unwrap();
    assert_eq!(d.failure.error.code, Code::FeederUnavailable);
    assert!(d.allows(Action::Retry) && d.allows(Action::Skip) && d.allows(Action::Defer));
    assert!(!d.allows(Action::SwitchOffFeeder));
}

#[test]
fn an_action_with_no_stopped_job_is_refused_and_told_in_the_log() {
    let mut a = app("");
    assert!(!a.decide(Action::Retry));
    a.step();
    assert!(!a.decide(Action::Skip));
    let told = a
        .log
        .entries()
        .iter()
        .filter(|e| e.text.contains(&Code::JobActionBad.label()))
        .count();
    assert_eq!(told, 2);
}

// ---- the steps and the tab of a picture

#[test]
fn the_steps_of_a_picture_stop_when_the_job_finishes_or_an_error_stops_it() {
    let mut a = app("");
    assert_eq!(a.run_steps(4), 4);
    assert_eq!(a.session.phase(), Some(Phase::Paused));
    assert_eq!(a.run_steps(100), 8);
    assert_eq!(a.session.phase(), Some(Phase::Finished));
    assert_eq!(a.run_steps(5), 0);
    let mut failing = app(FAILS);
    assert_eq!(failing.run_steps(10), 1);
    assert_eq!(failing.session.phase(), Some(Phase::Stopped));
    assert_eq!(app("").run_steps(0), 0);
}

#[test]
fn the_tabs_have_names_on_the_command_line() {
    for tab in Tab::ALL {
        assert_eq!(Tab::from_key(tab.key()), Some(tab));
        assert_eq!(Tab::from_key(&tab.key().to_uppercase()), Some(tab));
    }
    assert_eq!(Tab::from_key("nothing"), None);
    assert_eq!(Tab::from_key(" job "), Some(Tab::Job));
}

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn the_picture_can_ask_for_a_tab_and_for_steps_of_the_job() {
    let ok = parse_window_args(
        &args(&[
            "--shot", "job.png", "--window", "Job", "--steps", "7", "--open", "a.csv",
        ]),
        4,
    )
    .unwrap();
    assert_eq!(ok.window, Some(Tab::Job));
    assert_eq!(ok.steps, Some(7));
    assert_eq!(ok.files.len(), 1);
    assert!(ok.shot.is_some());
    let none = parse_window_args(&args(&["--shot", "job.png"]), 1).unwrap();
    assert_eq!((none.window, none.steps), (None, None));
    // the limits and the mistakes
    let top = STEPS_MAX.to_string();
    assert!(parse_window_args(&args(&["--shot", "x.png", "--steps", &top]), 1).is_ok());
    for bad in [
        &["--shot", "x.png", "--steps", "100001"][..],
        &["--shot", "x.png", "--steps", "-1"],
        &["--shot", "x.png", "--steps", "many"],
        &["--shot", "x.png", "--steps"],
        &["--shot", "x.png", "--window", "desk"],
        &["--shot", "x.png", "--window"],
        &["--window", "job"],
        &["--steps", "3"],
    ] {
        assert!(parse_window_args(&args(bad), 1).is_err(), "{bad:?}");
    }
    let e = parse_window_args(&args(&["--window", "job"]), 1).unwrap_err();
    assert!(e.contains("--shot"), "{e}");
}
