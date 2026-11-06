//! The align step: after a part is picked the camera looks at it, the place is corrected by what it sees, a part that is too far off is thrown away, and the time, the statistics and the
//! saved state of the job follow. With the step off (the default) a job is exactly what it was.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::{Code, Lang};
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::job::{Job, Phase, STATS_CSV_HEADER};
use eh3d_phoenix_pnp::model::{Camera, FeederDevice, Machine, Nozzle, Offset};
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use eh3d_phoenix_pnp::sim::{Rng, SimCamera};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Output};

const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nR3,10k,R,30,10,180,top\nC1,100n,C,10,20,0,top\nC2,100n,C,20,20,0,top\nU1,MCU,Q,40,15,0,top\n";
const FEEDERS: &str =
    "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\n";

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

fn texts(j: &Job) -> Vec<String> {
    j.machine().events.iter().map(|e| e.text.clone()).collect()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn code_of(text: &str) -> Code {
    MachineConfig::parse(text).unwrap_err().code
}

// ---- a job without the step is the job it always was

#[test]
fn with_the_step_off_nothing_of_a_job_changes_whatever_the_other_settings_say() {
    let plain = {
        let mut j = job("");
        j.run(None).unwrap();
        j
    };
    // every setting of the step is set to something that would change a job, but the step is off
    let mut noisy = job(
        "align = off\npick_tolerance_mm = 0.01\nalign_passes = 5\nalign_settle_s = 9\nsim_camera_fail_rate = 1\nsim_align_dx_mm = 40\nsim_align_dy_mm = -40\nsim_align_deg = 90\nsim_align_noise_mm = 5\n",
    );
    noisy.run(None).unwrap();
    assert_eq!(texts(&plain), texts(&noisy));
    assert_eq!(plain.stats_json(), noisy.stats_json());
    assert_eq!(plain.stats_csv(), noisy.stats_csv());
    assert_eq!(plain.save_state(), noisy.save_state());
    assert_eq!(plain.hash(), noisy.hash());
    assert_eq!(plain.machine().now_us(), noisy.machine().now_us());
    assert_eq!(plain.plan().to_json(), noisy.plan().to_json());
    assert!(near(noisy.plan().align_s, 0.0));
    // and nothing of the step is written anywhere
    assert!(!plain.stats_json().contains("align"));
    assert!(!plain.stats_csv().contains("align"));
    assert!(!plain.save_state().contains("align"));
    assert!(!plain.plan().to_json().contains("align"));
    assert_eq!(noisy.stats().align.corrected, 0);
}

// ---- the correction is applied to the place

const OFFSET: &str =
    "align = on\nsim_align_dx_mm = 0.2\nsim_align_dy_mm = -0.1\nsim_align_deg = 3\n";

#[test]
fn the_place_is_moved_and_turned_by_what_the_camera_sees() {
    let mut j = job(OFFSET);
    let first = j.plan().plan.placements[0].clone();
    j.step().unwrap();
    assert_eq!(j.machine().holding(0), Some(first.reference.as_str()));
    // the part is 0.2 mm to the right of the nozzle and 0.1 mm below it, turned 3 degrees: the head goes 0.2 to the left and 0.1 up of the place, and the nozzle turns 3 less
    j.step().unwrap();
    let (x, y) = j.machine().position();
    assert!(near(x, first.x - 0.2), "{x} {}", first.x);
    assert!(near(y, first.y + 0.1), "{y} {}", first.y);
    let texts = texts(&j);
    let looked = "look 1 at nozzle 1: X 0.200 Y -0.100 mm, turned 3.000 degrees";
    assert!(texts.iter().any(|t| t == looked), "{texts:?}");
    let turned = format!(
        "nozzle 1 places {} turned {:.3} degrees",
        first.reference,
        first.rotation - 3.0
    );
    assert!(texts.iter().any(|t| *t == turned), "{texts:?}");
    // the correction is for one part: the next one is looked at again
    assert_eq!(j.stats().align.corrected, 1);
}

#[test]
fn every_part_is_counted_with_its_distance_and_turn() {
    let mut j = job(OFFSET);
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    let a = &j.stats().align;
    // the distance of every part is the square root of 0.2 squared plus 0.1 squared, 0.223606797749979 mm
    assert_eq!((a.corrected, a.rejected, a.camera_failures), (6, 0, 0));
    assert!(near(a.mean_mm(), 0.223_606_797_749_979), "{}", a.mean_mm());
    assert!(near(a.max_mm, 0.223_606_797_749_979), "{}", a.max_mm);
    assert!(near(a.max_deg, 3.0), "{}", a.max_deg);
    let s: Value = serde_json::from_str(&j.stats_json()).unwrap();
    assert_eq!(s["align"]["corrected"], 6);
    assert_eq!(s["align"]["rejected"], 0);
    assert_eq!(s["align"]["passes"], 1);
    assert!(near(s["align"]["max_deg"].as_f64().unwrap(), 3.0));
    assert!(near(s["align"]["pick_tolerance_mm"].as_f64().unwrap(), 0.5));
}

#[test]
fn further_passes_find_nothing_more_to_correct_when_the_first_was_exact() {
    let mut one = job(OFFSET);
    one.run(None).unwrap();
    let mut three = job(&format!("{OFFSET}align_passes = 3\n"));
    three.run(None).unwrap();
    // the second and the third look find the part where the first correction put it: the same total, at the same places
    assert_eq!(one.stats().align, three.stats().align);
    let looks = texts(&three)
        .iter()
        .filter(|t| t.starts_with("look 3 at nozzle 1: X 0.000 Y 0.000 mm, turned 0.000 degrees"))
        .count();
    assert_eq!(looks, 6);
    let places = |j: &Job| -> Vec<String> {
        texts(j)
            .into_iter()
            .filter(|t| t.starts_with("nozzle 1 places"))
            .collect()
    };
    assert_eq!(places(&one), places(&three));
}

// ---- the time

#[test]
fn the_step_adds_the_waits_of_every_look_to_the_plan_and_to_the_machine_clock() {
    let mut off = job("");
    off.run(None).unwrap();
    // six parts, two looks each, a quarter of a second before every look: 6 * 2 * 0.25 = 3 seconds
    let cfg = "align = on\nalign_passes = 2\nalign_settle_s = 0.25\n";
    let mut on = job(cfg);
    on.run(None).unwrap();
    assert_eq!(on.machine().now_us(), off.machine().now_us() + 3_000_000);
    assert!(near(on.plan().align_s, 3.0), "{}", on.plan().align_s);
    assert!(near(on.plan().plan.seconds, off.plan().plan.seconds + 3.0));
    assert!(on.plan().to_json().contains("\"align_s\": 3.0"));
    // a wait of 0 adds nothing
    let mut none = job("align = on\nalign_settle_s = 0\n");
    none.run(None).unwrap();
    assert_eq!(none.machine().now_us(), off.machine().now_us());
    assert!(!none.plan().to_json().contains("align_s"));
}

// ---- the tolerance

const FAR: &str = "align = on\nsim_align_dx_mm = 0.6\npick_tolerance_mm = 0.5\nfault_limit = 0\n";

#[test]
fn a_part_further_than_the_tolerance_is_thrown_away_and_the_policy_decides() {
    // defer: two attempts for each of the six parts (pick_retries = 1), every one thrown away
    let mut j = job(&format!("{FAR}error_policy = defer\npick_retries = 1\n"));
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    let s = j.stats();
    assert_eq!((s.placed, s.deferred), (0, 6));
    assert_eq!((s.align.corrected, s.align.rejected), (0, 12));
    assert_eq!(s.pick_attempts, 12);
    assert_eq!(j.machine().holding(0), None);
    let aside = j.deferred();
    assert_eq!(aside.len(), 6);
    assert!(
        aside[0].1.starts_with("PHOENIX-PNP-035: "),
        "{}",
        aside[0].1
    );
    assert!(aside[0].1.contains("0.600 mm"), "{}", aside[0].1);
    // every thrown-away part is gone from its feeder: 3 resistors twice from F1 (10 - 6), 2 capacitors twice from F2 (10 - 4), the chip twice from F3 (2 - 2)
    let left: Vec<u32> = j.devices_mut().iter().map(|d| d.remaining()).collect();
    assert_eq!(left, [4, 6, 0]);
    let thrown = texts(&j)
        .iter()
        .filter(|t| t.ends_with("mm from the nozzle: thrown away"))
        .count();
    assert_eq!(thrown, 12);
}

#[test]
fn with_alert_the_job_stops_at_the_part_and_resume_looks_again() {
    let mut j = job(&format!("{FAR}pick_retries = 0\n"));
    let e = j.run(None).unwrap_err();
    assert_eq!(e.code, Code::AlignRejected);
    assert_eq!(e.code.label(), "PHOENIX-PNP-035");
    assert_eq!(j.phase(), Phase::Stopped);
    assert_eq!(j.machine().holding(0), None);
    assert_eq!(j.stats().align.rejected, 1);
    j.resume();
    assert_eq!(j.run(None).unwrap_err().code, Code::AlignRejected);
    assert_eq!(j.stats().align.rejected, 2);
}

#[test]
fn a_part_exactly_at_the_tolerance_is_good() {
    let mut j = job("align = on\nsim_align_dx_mm = 0.5\npick_tolerance_mm = 0.5\n");
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    assert_eq!(
        (j.stats().align.corrected, j.stats().align.rejected),
        (6, 0)
    );
    // the tolerance is in distance, not in each axis: 0.4 and 0.4 are 0.5657 mm apart
    let mut far = job(
        "align = on\nsim_align_dx_mm = 0.4\nsim_align_dy_mm = 0.4\npick_tolerance_mm = 0.5\npick_retries = 0\nerror_policy = defer\n",
    );
    far.run(None).unwrap();
    assert_eq!(far.stats().align.rejected, 6);
}

// ---- the failures of the camera

#[test]
fn a_camera_that_gives_no_reading_throws_the_part_away_and_is_counted() {
    let mut j = job(
        "align = on\nalign_passes = 3\nsim_camera_fail_rate = 1\nerror_policy = defer\npick_retries = 0\nfault_limit = 0\n",
    );
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    let s = j.stats();
    assert_eq!((s.placed, s.deferred), (0, 6));
    // the first look of every part fails and the other two are not made
    assert_eq!(
        (s.align.camera_failures, s.align.rejected, s.align.corrected),
        (6, 0, 0)
    );
    assert!(j.deferred()[0].1.starts_with("PHOENIX-PNP-036: "));
    // the failure of a look does not move the failures of the machine: the picks were all good
    assert_eq!(s.pick_failures, 0);
}

#[test]
fn a_look_that_a_test_makes_fail_is_made_again_with_the_next_part() {
    let mut j = job("align = on\npick_retries = 1\n");
    j.camera_mut().force_fail(1);
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    let s = j.stats();
    assert_eq!(s.placed, 6);
    assert_eq!(
        (s.align.camera_failures, s.align.corrected, s.pick_attempts),
        (1, 6, 7)
    );
    // seven parts left the feeders: 22 - 7
    let left: u32 = j.devices_mut().iter().map(|d| d.remaining()).sum();
    assert_eq!(left, 15);
}

#[test]
fn the_camera_has_a_generator_of_its_own_so_the_machine_fails_in_the_same_places() {
    // the same seed, with and without an error in the readings: the pick and place failures of the machine are the same
    let base = "sim_pick_fail_rate = 0.3\nsim_place_fail_rate = 0.2\nsim_seed = 9\nerror_policy = defer\nfault_limit = 0\n";
    let mut a = job(&format!("{base}align = on\n"));
    let mut b = job(&format!(
        "{base}align = on\nsim_camera_fail_rate = 0\nsim_align_noise_mm = 0.1\n"
    ));
    a.run(None).unwrap();
    b.run(None).unwrap();
    assert_eq!(a.stats().pick_failures, b.stats().pick_failures);
    assert_eq!(a.stats().place_failures, b.stats().place_failures);
    assert_eq!(a.stats().placed, b.stats().placed);
}

// ---- the camera of the simulator

#[test]
fn the_simulated_camera_sees_the_part_less_what_was_corrected() {
    let mut cam = SimCamera::new(Offset {
        dx_mm: 0.5,
        dy_mm: -0.25,
        rotation_deg: 2.0,
    });
    assert_eq!(
        cam.measure(0).unwrap(),
        Offset {
            dx_mm: 0.5,
            dy_mm: -0.25,
            rotation_deg: 2.0
        }
    );
    cam.apply(Offset {
        dx_mm: 0.25,
        dy_mm: -0.125,
        rotation_deg: 1.0,
    });
    assert_eq!(
        cam.measure(0).unwrap(),
        Offset {
            dx_mm: 0.25,
            dy_mm: -0.125,
            rotation_deg: 1.0
        }
    );
    // a new part: nothing has been corrected
    cam.clear();
    assert_eq!(cam.measure(0).unwrap().dx_mm, 0.5);
}

#[test]
fn the_simulated_camera_fails_at_the_look_that_is_chosen_and_by_chance() {
    let mut cam = SimCamera::default();
    cam.force_fail(2);
    assert!(cam.measure(0).is_ok());
    assert_eq!(cam.measure(0).unwrap_err().code, Code::SimFault);
    assert!(cam.measure(0).is_ok());
    let mut always = SimCamera::new(Offset::default());
    always.fail_rate = 1.0;
    for _ in 0..20 {
        assert_eq!(always.measure(0).unwrap_err().code, Code::SimFault);
    }
}

#[test]
fn the_error_of_a_reading_comes_from_the_seed_and_stays_inside_its_bounds() {
    let cfg =
        MachineConfig::parse("sim_seed = 1\nsim_align_noise_mm = 0.1\nsim_align_dx_mm = 0.25\n")
            .unwrap();
    let mut cam = SimCamera::from_config(&cfg);
    // the camera draws a number for the chance of failing and then one for each of dx, dy and the turn
    let mut r = Rng::new(1u64 ^ 0xA11C_E5ED_0000_0001);
    let _chance = r.next_f64();
    let (a, b, c) = (r.next_f64(), r.next_f64(), r.next_f64());
    let seen = cam.measure(0).unwrap();
    assert!(near(seen.dx_mm, 0.25 + (2.0 * a - 1.0) * 0.1), "{seen:?}");
    assert!(near(seen.dy_mm, (2.0 * b - 1.0) * 0.1), "{seen:?}");
    assert!(near(seen.rotation_deg, (2.0 * c - 1.0) * 0.1), "{seen:?}");
    // in a job every reading is inside the band, and another seed gives other readings
    let mut j = job("align = on\nsim_align_noise_mm = 0.1\nsim_seed = 4\n");
    j.run(None).unwrap();
    let diagonal: f64 = 0.1_f64.hypot(0.1);
    assert!(j.stats().align.max_mm <= diagonal + 1e-12);
    assert!(j.stats().align.max_mm > 0.0);
    assert!(j.stats().align.mean_mm() <= j.stats().align.max_mm + 1e-12);
    let mut k = job("align = on\nsim_align_noise_mm = 0.1\nsim_seed = 5\n");
    k.run(None).unwrap();
    assert_ne!(j.stats().align, k.stats().align);
    let mut again = job("align = on\nsim_align_noise_mm = 0.1\nsim_seed = 4\n");
    again.run(None).unwrap();
    assert_eq!(j.stats().align, again.stats().align);
    assert_eq!(texts(&j), texts(&again));
}

// ---- the saved state

const NOISY: &str = "align = on\nalign_passes = 2\nsim_align_dx_mm = 0.2\nsim_align_dy_mm = 0.1\nsim_align_deg = 2\nsim_align_noise_mm = 0.1\nsim_camera_fail_rate = 0.3\nsim_seed = 5\nerror_policy = defer\npick_retries = 1\nplace_retries = 1\nfault_limit = 0\n";

#[test]
fn a_saved_job_with_the_step_resumes_anywhere_with_the_same_corrections() {
    let mut whole = job(NOISY);
    whole.run(None).unwrap();
    assert!(whole.stats().align.corrected > 0);
    assert!(whole.stats().align.camera_failures > 0);
    for stop in 0..=14 {
        let mut first = job(NOISY);
        first.run(Some(stop)).unwrap();
        let mut second = job(NOISY);
        second.restore_state(&first.save_state()).unwrap();
        second.resume();
        second.run(None).unwrap();
        assert_eq!(second.stats(), whole.stats(), "stop {stop}");
        assert_eq!(second.stats_json(), whole.stats_json(), "stop {stop}");
        assert_eq!(second.stats_csv(), whole.stats_csv(), "stop {stop}");
        assert_eq!(second.machine().position(), whole.machine().position());
    }
}

#[test]
fn a_state_of_the_step_is_for_the_job_with_the_step_and_a_damaged_one_changes_nothing() {
    let mut on = job(OFFSET);
    on.run(Some(1)).unwrap();
    let state = on.save_state();
    // the part that the nozzle holds comes with the correction that was found for it
    let v: Value = serde_json::from_str(&state).unwrap();
    assert_eq!(v["align"]["corrections"].as_array().unwrap().len(), 1);
    assert!(v["align"]["corrections"][0].is_array());
    // a job without the step is another job
    let mut off = job("");
    assert_eq!(
        off.restore_state(&state).unwrap_err().code,
        Code::JobStateMismatch
    );
    for damage in [
        json!([]),
        json!([null, null]),
        json!([["1", "2"]]),
        json!([["x", "1", "2"]]),
        json!([5]),
    ] {
        let mut bad = v.clone();
        bad["align"]["corrections"] = damage;
        let mut j = job(OFFSET);
        let before = j.save_state();
        assert_eq!(
            j.restore_state(&bad.to_string()).unwrap_err().code,
            Code::JobStateBad
        );
        assert_eq!(j.save_state(), before);
    }
    let mut bad = v.clone();
    bad["align"]["sum_mm"] = json!("not a number");
    assert_eq!(
        job(OFFSET)
            .restore_state(&bad.to_string())
            .unwrap_err()
            .code,
        Code::JobStateBad
    );
    // a state saved with the step but without its section starts the step again from zero
    let mut plain = v.clone();
    plain.as_object_mut().unwrap().remove("align");
    let mut j = job(OFFSET);
    j.restore_state(&plain.to_string()).unwrap();
    assert_eq!(j.stats().align.corrected, 0);
}

// ---- the settings

#[test]
fn the_settings_of_the_step_have_their_defaults_and_strict_ranges() {
    let d = MachineConfig::parse("").unwrap();
    assert!(!d.align);
    assert!(near(d.pick_tolerance_mm, 0.5));
    assert_eq!(d.align_passes, 1);
    assert!(near(d.align_settle_s, 0.1));
    assert!(near(d.sim_camera_fail_rate, 0.0));
    assert!(near(d.sim_align_dx_mm, 0.0) && near(d.sim_align_dy_mm, 0.0));
    assert!(near(d.sim_align_deg, 0.0) && near(d.sim_align_noise_mm, 0.0));
    for yes in ["on", "yes", "true", "1", "ON"] {
        assert!(
            MachineConfig::parse(&format!("align = {yes}"))
                .unwrap()
                .align
        );
    }
    for no in ["off", "no", "false", "0"] {
        assert!(
            !MachineConfig::parse(&format!("align = {no}"))
                .unwrap()
                .align
        );
    }
    assert_eq!(code_of("align = maybe"), Code::ConfigBadValue);
    assert_eq!(code_of("align = on\nalign = off"), Code::ConfigDuplicateKey);
    // the edges are good and a step beyond them is not
    for (key, good, bad) in [
        ("pick_tolerance_mm", ["0.001", "50"], ["0", "50.1"]),
        ("align_passes", ["1", "5"], ["0", "6"]),
        ("align_settle_s", ["0", "10"], ["-0.1", "10.5"]),
        ("sim_camera_fail_rate", ["0", "1"], ["-0.1", "1.5"]),
        ("sim_align_dx_mm", ["-50", "50"], ["-50.1", "51"]),
        ("sim_align_dy_mm", ["-50", "50"], ["-50.1", "51"]),
        ("sim_align_deg", ["-180", "180"], ["-181", "181"]),
        ("sim_align_noise_mm", ["0", "10"], ["-1", "11"]),
    ] {
        for v in good {
            assert!(
                MachineConfig::parse(&format!("{key} = {v}")).is_ok(),
                "{key} {v}"
            );
        }
        for v in bad {
            assert_eq!(
                code_of(&format!("{key} = {v}")),
                Code::ConfigOutOfRange,
                "{key} {v}"
            );
        }
        assert_eq!(code_of(&format!("{key} = many")), Code::ConfigBadValue);
    }
    assert_eq!(code_of("align_passes = 2.5"), Code::ConfigBadValue);
}

// ---- the codes

#[test]
fn the_codes_35_and_36_have_a_meaning_in_every_language() {
    for code in [Code::AlignRejected, Code::AlignFailed] {
        let mut seen = std::collections::HashSet::new();
        for lang in Lang::ALL {
            let m = code.message(lang);
            assert!(!m.is_empty());
            seen.insert(m);
        }
        assert_eq!(seen.len(), 8, "{}", code.label());
    }
    assert_eq!(Code::AlignRejected.label(), "PHOENIX-PNP-035");
    assert_eq!(Code::AlignFailed.label(), "PHOENIX-PNP-036");
    assert_eq!(Code::ALL.len(), 36);
    assert_eq!(
        Code::AlignRejected.message(Lang::Pt),
        "A câmara encontrou a peça mais deslocada do que a tolerância permite"
    );
    assert_eq!(
        Code::AlignFailed.message(Lang::Pt),
        "A câmara não deu nenhuma leitura da peça"
    );
    let e = Code::AlignRejected.message(Lang::Es);
    assert!(e.contains("tolerancia"), "{e}");
}

// ---- the command line and the report

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

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn simulate_reports_the_step_in_the_text_and_in_a_section_of_the_csv() {
    let dir = std::env::temp_dir().join(format!("eh3d_pnp_align_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("align.cfg");
    let csv = dir.join("job.csv");
    std::fs::write(
        &cfg,
        "align = on\nalign_passes = 2\nalign_settle_s = 0.25\npick_tolerance_mm = 1\nsim_align_dx_mm = 0.3\nsim_align_dy_mm = 0.4\n",
    )
    .unwrap();
    let (cfg, csv) = (
        cfg.to_string_lossy().into_owned(),
        csv.to_string_lossy().into_owned(),
    );
    let o = run(&[
        "simulate",
        &example("board.pos"),
        &example("feeders.csv"),
        "--config",
        &cfg,
        "--report-csv",
        &csv,
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out = String::from_utf8_lossy(&o.stdout);
    let line = "align: 6 parts corrected (mean 0.500 mm, largest 0.500 mm, largest turn 0.00 degrees), 0 thrown away over 1 mm, 0 looks without a reading";
    assert!(out.lines().any(|l| l == line), "{out}");
    let text = std::fs::read_to_string(&csv).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    // the header is the one that never changes and every row has its nine columns
    assert_eq!(lines[0], STATS_CSV_HEADER);
    for l in &lines {
        assert_eq!(l.split(',').count(), 9, "{l}");
    }
    // the rows of the step form a section of their own, after all the others
    let first = lines.iter().position(|l| l.starts_with("align,")).unwrap();
    assert!(lines[first..].iter().all(|l| l.starts_with("align,")));
    assert!(lines[..first].iter().all(|l| !l.starts_with("align,")));
    let section: Vec<&str> = lines[first..].to_vec();
    assert_eq!(
        section,
        [
            "align,passes,2,,,,,,",
            "align,pick_tolerance_mm,1.000,,,,,,",
            "align,corrected,6,,,,,,",
            "align,rejected,0,,,,,,",
            "align,camera_failures,0,,,,,,",
            "align,mean_mm,0.500,,,,,,",
            "align,max_mm,0.500,,,,,,",
            "align,max_deg,0.000,,,,,,",
        ]
    );
    // and a job without the step has no such section, nor such a line
    let plain = dir.join("plain.csv").to_string_lossy().into_owned();
    let o = run(&[
        "simulate",
        &example("board.pos"),
        &example("feeders.csv"),
        "--report-csv",
        &plain,
    ]);
    assert!(o.status.success());
    assert!(!String::from_utf8_lossy(&o.stdout).contains("align"));
    assert!(!std::fs::read_to_string(&plain).unwrap().contains("align"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_text_report_of_a_plan_says_how_long_the_camera_takes() {
    let dir = std::env::temp_dir().join(format!("eh3d_pnp_align_plan_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("align.cfg");
    std::fs::write(
        &cfg,
        "align = on\nalign_passes = 2\nalign_settle_s = 0.25\n",
    )
    .unwrap();
    let (cfg, plan, report) = (
        cfg.to_string_lossy().into_owned(),
        dir.join("plan.json").to_string_lossy().into_owned(),
        dir.join("report.txt").to_string_lossy().into_owned(),
    );
    let o = run(&[
        "schedule",
        &example("board.pos"),
        &example("feeders.csv"),
        &plan,
        "--config",
        &cfg,
        "--report-text",
        &report,
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let text = std::fs::read_to_string(&report).unwrap();
    assert!(
        text.contains("of which 3.0 s for the camera to look at the parts"),
        "{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
