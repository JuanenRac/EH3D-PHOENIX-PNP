//! The extra looks of the camera at a part that was over the tolerance (`align_rereads`) and the counters of the looks for each feeder: the setting, the effect on the job, the time, the saved
//! state, and the work report in its text, its JSON and its CSV.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::error::{Code, Lang};
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::job::{Job, Phase};
use eh3d_phoenix_pnp::model::Machine;
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use eh3d_phoenix_pnp::report::{build, csv, json, ReportInput};
use serde_json::Value;

const MACHINE: &str = include_str!("../examples/machine.cfg");
const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nR3,10k,R,30,10,180,top\nC1,100n,C,10,20,0,top\nC2,100n,C,20,20,0,top\nU1,MCU,Q,40,15,0,top\n";
const FEEDERS: &str =
    "id,value,package,x,y,count\nF1,10k,R,-20,10,40\nF2,100n,C,-20,20,40\nF3,MCU,Q,-20,40,40\n";
/// A camera whose readings scatter around 0.4 mm with a spread of 0.4 mm on each axis, a tolerance of 0.5 mm, and a job that puts a part aside rather than stop.
const SCATTER: &str = "align = on\nsim_align_dx_mm = 0.4\nsim_align_noise_mm = 0.4\npick_tolerance_mm = 0.5\nerror_policy = defer\npick_retries = 0\nfault_limit = 0\nalign_settle_s = 0.2\n";
/// A camera that sees every part a millimetre off, whatever the look.
const ALWAYS_OVER: &str = "align = on\nsim_align_dx_mm = 1\npick_tolerance_mm = 0.5\nerror_policy = defer\npick_retries = 0\nfault_limit = 0\nalign_settle_s = 0.2\n";

fn job(cfg: &str) -> Job {
    let cfg = MachineConfig::parse(cfg).unwrap();
    let parts = from_csv(PARTS).unwrap();
    let feeders = feeding::from_csv(FEEDERS).unwrap();
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

fn finished(cfg: &str) -> Job {
    let mut j = job(cfg);
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    j
}

fn code_of(text: &str) -> Code {
    MachineConfig::parse(text).unwrap_err().code
}

// ---- the setting

#[test]
fn the_extra_looks_are_a_setting_that_is_off_by_default() {
    let d = MachineConfig::default();
    assert_eq!(d.align_rereads, 0);
    assert!(KEYS.contains(&"align_rereads"));
    assert!(MACHINE.contains("\nalign_rereads = 0"));
    assert_eq!(MachineConfig::parse(MACHINE).unwrap().align_rereads, 0);
    for ok in ["0", "1", "5"] {
        let c = MachineConfig::parse(&format!("align_rereads = {ok}")).unwrap();
        assert_eq!(c.align_rereads.to_string(), ok);
    }
    for bad in ["-1", "6"] {
        assert_eq!(
            code_of(&format!("align_rereads = {bad}")),
            Code::ConfigOutOfRange,
            "{bad}"
        );
    }
    for bad in ["2.5", "many"] {
        assert_eq!(
            code_of(&format!("align_rereads = {bad}")),
            Code::ConfigBadValue,
            "{bad}"
        );
    }
}

// ---- the job

#[test]
fn without_extra_looks_a_part_over_the_tolerance_is_thrown_away_at_the_first_look() {
    let j = finished(ALWAYS_OVER);
    let s = j.stats();
    assert_eq!(s.align.rereads, 0);
    assert_eq!(s.align.recovered, 0);
    assert_eq!(s.align.rejected, 6);
    for f in &s.feeders {
        assert_eq!((f.rereads, f.recovered), (0, 0), "{}", f.id);
        assert_eq!(f.looks_over, f.thrown_away, "{}", f.id);
        assert_eq!(f.no_reading, 0);
    }
    assert_eq!(s.feeders.iter().map(|f| f.thrown_away).sum::<u32>(), 6);
}

#[test]
fn each_extra_look_adds_to_the_time_and_a_part_that_stays_over_is_still_thrown_away() {
    let none = finished(ALWAYS_OVER);
    let two = finished(&format!("{ALWAYS_OVER}align_rereads = 2\n"));
    let s = two.stats();
    // every part is looked at three times and thrown away
    assert_eq!(s.align.rejected, 6);
    assert_eq!(s.align.rereads, 12);
    assert_eq!(s.align.recovered, 0);
    for f in &s.feeders {
        assert_eq!(f.rereads, 2 * f.looks_over, "{}", f.id);
        assert_eq!(f.looks_over, f.thrown_away);
        assert_eq!(f.recovered, 0);
    }
    // twelve more looks of 0.2 seconds
    let more = two.trace().align_us - none.trace().align_us;
    assert_eq!(more, 12 * 200_000, "{more}");
    assert!(two.machine().now_us() > none.machine().now_us());
    // nothing was placed in either
    assert_eq!(two.stats().placed, 0);
}

#[test]
fn a_second_look_can_save_a_part_and_every_part_over_is_saved_or_thrown_away() {
    let (mut thrown_without, mut thrown_with, mut saved) = (0, 0, 0);
    for seed in 1..=8 {
        let without = finished(&format!("{SCATTER}sim_seed = {seed}\n"));
        let with = finished(&format!("{SCATTER}sim_seed = {seed}\nalign_rereads = 3\n"));
        thrown_without += without.stats().align.rejected;
        thrown_with += with.stats().align.rejected;
        saved += with.stats().align.recovered;
        for j in [&without, &with] {
            let s = j.stats();
            for f in &s.feeders {
                // no look gave nothing here, so a part over at the first look was saved or thrown away
                assert_eq!(f.looks_over, f.recovered + f.thrown_away, "{}", f.id);
                assert!(f.rereads <= 3 * f.looks_over, "{}", f.id);
                assert!(f.rereads >= f.recovered, "{}", f.id);
            }
            assert_eq!(
                s.align.rereads,
                s.feeders.iter().map(|f| f.rereads).sum::<u32>()
            );
            assert_eq!(
                s.align.recovered,
                s.feeders.iter().map(|f| f.recovered).sum::<u32>()
            );
            assert_eq!(
                s.align.rejected,
                s.feeders.iter().map(|f| f.thrown_away).sum::<u32>()
            );
        }
        assert_eq!(without.stats().align.rereads, 0);
    }
    assert!(thrown_without > 0, "the scatter should put parts over");
    assert!(saved > 0, "a second look should save a part now and then");
    assert!(
        thrown_with < thrown_without,
        "{thrown_with} against {thrown_without}"
    );
}

#[test]
fn a_look_without_a_reading_is_counted_for_the_feeder() {
    let j = finished(
        "align = on\nsim_camera_fail_rate = 1\nerror_policy = defer\npick_retries = 0\nfault_limit = 0\n",
    );
    let s = j.stats();
    assert_eq!(s.align.camera_failures, 6);
    assert_eq!(s.feeders.iter().map(|f| f.no_reading).sum::<u32>(), 6);
    assert!(s.feeders.iter().all(|f| f.looks_over == 0));
    // a failing look in the middle of the extra looks ends the part too, and is counted
    let j = finished(&format!(
        "{ALWAYS_OVER}align_rereads = 2\nsim_camera_fail_rate = 0.5\nsim_seed = 3\n"
    ));
    let s = j.stats();
    assert_eq!(
        s.feeders.iter().map(|f| f.no_reading).sum::<u32>(),
        s.align.camera_failures
    );
}

#[test]
fn with_the_step_off_the_new_setting_changes_nothing() {
    let plain = finished("");
    let set = finished("align_rereads = 4\nsim_align_dx_mm = 3\n");
    assert_eq!(plain.stats(), set.stats());
    assert_eq!(plain.machine().now_us(), set.machine().now_us());
}

// ---- the saved state

#[test]
fn the_counters_are_saved_and_a_state_from_before_them_starts_them_at_zero() {
    let cfg = format!("{SCATTER}sim_seed = 4\nalign_rereads = 3\n");
    let mut j = job(&cfg);
    j.run(Some(12)).unwrap();
    let saved = j.save_state();
    let mut back = job(&cfg);
    back.restore_state(&saved).unwrap();
    assert_eq!(back.stats().align, j.stats().align);
    assert_eq!(back.stats().feeders, j.stats().feeders);
    // the run goes on to the same end from the state that was saved
    j.run(None).unwrap();
    back.run(None).unwrap();
    assert_eq!(back.stats(), j.stats());
    assert!(j.stats().align.rereads > 0);
    // a state saved before the counters existed has none of the keys
    let mut doc: Value = serde_json::from_str(&saved).unwrap();
    for f in doc["stats"]["feeders"].as_array_mut().unwrap() {
        let o = f.as_object_mut().unwrap();
        for k in [
            "looks_over",
            "rereads",
            "recovered",
            "thrown_away",
            "no_reading",
        ] {
            assert!(o.remove(k).is_some(), "{k}");
        }
    }
    let a = doc["align"].as_object_mut().unwrap();
    a.remove("rereads").unwrap();
    a.remove("recovered").unwrap();
    let mut old = job(&cfg);
    old.restore_state(&doc.to_string()).unwrap();
    assert_eq!(old.stats().align.rereads, 0);
    assert_eq!(old.stats().align.recovered, 0);
    assert!(old.stats().feeders.iter().all(|f| !f.has_looks()));
}

// ---- the report

#[test]
fn the_report_counts_the_looks_of_each_feeder_in_its_text_json_and_csv() {
    let j = finished(&format!("{ALWAYS_OVER}align_rereads = 1\n"));
    let r = build(&j, &ReportInput::default());
    assert_eq!((r.rereads, r.recovered), (6, 0));
    let text = r.text(Lang::En);
    assert!(text.contains("Camera by feeder"), "{text}");
    assert!(
        text.contains("Extra looks at a part over the tolerance"),
        "{text}"
    );
    assert!(text.contains("Parts saved by an extra look"), "{text}");
    for lang in Lang::ALL {
        let t = r.text(lang);
        assert!(t.contains("F1"), "{t}");
    }
    let es = r.text(Lang::Es);
    assert!(es.contains("Cámara por alimentador"), "{es}");
    let doc = r.json();
    assert_eq!(doc["camera"]["rereads"], 6);
    assert_eq!(doc["camera"]["recovered"], 0);
    let feeders = doc["feeders"].as_array().unwrap();
    let f1 = feeders.iter().find(|f| f["id"] == "F1").unwrap();
    assert_eq!(f1["looks_over"], f1["thrown_away"]);
    assert_eq!(f1["rereads"], f1["looks_over"]);
    assert_eq!(f1["recovered"], 0);
    assert_eq!(f1["no_reading"], 0);
    // the CSV: a row for each feeder the camera had something to tell about, before the section of the align step
    let rows = csv(&j, &ReportInput::default());
    let look: Vec<&str> = rows.lines().filter(|l| l.starts_with("look,")).collect();
    assert_eq!(look.len(), 3, "{rows}");
    let f1_row = look.iter().find(|l| l.starts_with("look,F1,")).unwrap();
    let n = f1["looks_over"].as_u64().unwrap();
    assert_eq!(*f1_row, format!("look,F1,{n},{n},0,{n},0,,"));
    let at_look = rows.find("\nlook,").unwrap();
    let at_align = rows.find("\nalign,").unwrap();
    assert!(at_look < at_align);
    // the whole JSON of the job carries it too
    let all: Value = serde_json::from_str(&json(&j, &ReportInput::default())).unwrap();
    assert_eq!(all["work"]["camera"]["rereads"], 6);
}

#[test]
fn a_job_the_camera_had_nothing_to_tell_about_has_no_new_lines() {
    let j = finished("align = on\n");
    let r = build(&j, &ReportInput::default());
    assert_eq!((r.rereads, r.recovered), (0, 0));
    let text = r.text(Lang::En);
    assert!(!text.contains("Camera by feeder"), "{text}");
    assert!(!text.contains("Extra looks"), "{text}");
    assert!(!csv(&j, &ReportInput::default()).contains("\nlook,"));
    // with the step off there is no camera section at all
    let off = build(&finished(""), &ReportInput::default());
    assert!(!off.text(Lang::En).contains("Camera by feeder"));
    // the numbers of every feeder are in the JSON all the same
    let f = &off.json()["feeders"][0];
    assert_eq!(f["looks_over"], 0);
    assert_eq!(f["no_reading"], 0);
}
