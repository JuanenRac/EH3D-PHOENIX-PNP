//! The vision in the job and on the command line: the simulated camera measures a picture of each part (`vision = pipeline`), the failures of the vision are looks without a reading with their
//! own codes, the job saved in the middle resumes with the same readings, and `vision-test` and `simulate --vision` run as a person would run them.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::job::{Job, Phase};
use eh3d_phoenix_pnp::model::{Machine, Nozzle};
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use eh3d_phoenix_pnp::vision::VisionConfig;
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

fn near(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

const SEEN: &str = "align = on\nvision = pipeline\nsim_align_dx_mm = 0.3\nsim_align_dy_mm = -0.2\nsim_align_deg = 5\n";

// ---- a job with the vision off is the job it was

#[test]
fn the_vision_changes_nothing_unless_the_align_step_looks_at_the_camera() {
    let mut plain = job("");
    plain.run(None).unwrap();
    let mut pipeline = job("vision = pipeline\nsim_align_dx_mm = 9\n");
    pipeline.run(None).unwrap();
    assert_eq!(texts(&plain), texts(&pipeline));
    assert_eq!(plain.stats_json(), pipeline.stats_json());
    assert_eq!(plain.stats_csv(), pipeline.stats_csv());
    assert_eq!(plain.save_state(), pipeline.save_state());
    assert_eq!(plain.machine().now_us(), pipeline.machine().now_us());
    // and with the step on but the vision off, the camera reports the offset it was given, as it did
    let mut direct = job("align = on\nvision = off\nsim_align_dx_mm = 0.3\n");
    direct.run(None).unwrap();
    assert!(texts(&direct)
        .iter()
        .any(|t| t == "look 1 at nozzle 1: X 0.300 Y 0.000 mm, turned 0.000 degrees"));
}

// ---- the camera that measures a picture

#[test]
fn the_place_is_corrected_by_what_the_pipeline_measures() {
    let mut j = job(SEEN);
    let first = j.plan().plan.placements[0].clone();
    j.step().unwrap();
    assert_eq!(j.machine().holding(0), Some(first.reference.as_str()));
    j.step().unwrap();
    // 0.05 mm per pixel: the middle of the part is found to half a pixel, 0.025 mm
    let (x, y) = j.machine().position();
    assert!(near(x, first.x - 0.3, 0.03), "{x} {}", first.x);
    assert!(near(y, first.y + 0.2, 0.03), "{y} {}", first.y);
    assert!(texts(&j)
        .iter()
        .any(|t| t.starts_with("look 1 at nozzle 1: X ")));
}

#[test]
fn every_part_is_counted_with_the_distance_and_turn_that_were_measured() {
    let mut j = job(SEEN);
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    let a = &j.stats().align;
    assert_eq!((a.corrected, a.rejected, a.camera_failures), (6, 0, 0));
    // the distance is the square root of 0.3 squared plus 0.2 squared, 0.3606 mm, and the turn 5 degrees; a pixel is 0.05 mm and the turn of a part 32 pixels long is known to 1.8 degrees
    assert!(
        near(a.mean_mm(), 0.360_555_127_546_398_9, 0.04),
        "{}",
        a.mean_mm()
    );
    assert!(
        near(a.max_mm, 0.360_555_127_546_398_9, 0.04),
        "{}",
        a.max_mm
    );
    assert!(near(a.max_deg, 5.0, 1.9), "{}", a.max_deg);
    assert_eq!(j.stats().placed, 6);
}

#[test]
fn a_second_look_finds_the_part_where_the_first_correction_put_it() {
    let mut j = job(&format!("{SEEN}align_passes = 2\n"));
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    let looks: Vec<String> = texts(&j)
        .into_iter()
        .filter(|t| t.starts_with("look 2 at nozzle"))
        .collect();
    assert_eq!(looks.len(), 6);
    // what is left to find after the first correction is within the pixel: X and Y under 0.05 mm
    for l in looks {
        let words: Vec<&str> = l.split_whitespace().collect();
        let x: f64 = words[6].parse().unwrap();
        let y: f64 = words[8].parse().unwrap();
        assert!(x.abs() <= 0.05 && y.abs() <= 0.05, "{l}");
    }
}

#[test]
fn the_same_job_gives_the_same_readings_and_a_noise_below_the_threshold_changes_none() {
    let mut a = job(SEEN);
    let mut b = job(SEEN);
    a.run(None).unwrap();
    b.run(None).unwrap();
    assert_eq!(texts(&a), texts(&b));
    // the part is 200, the background 30 and the threshold 128: up to 60 levels of noise cannot cross it
    let mut noisy = job(SEEN);
    noisy.set_vision(VisionConfig {
        sim_noise_levels: 60,
        ..VisionConfig::default()
    });
    noisy.run(None).unwrap();
    assert_eq!(a.stats().align, noisy.stats().align);
    assert_eq!(texts(&a), texts(&noisy));
}

// ---- the failures of the vision

fn aside(j: &Job) -> Vec<String> {
    j.deferred().into_iter().map(|(_, why)| why).collect()
}

const GIVE_UP: &str = "error_policy = defer\npick_retries = 0\nfault_limit = 0\n";

#[test]
fn a_part_out_of_the_picture_is_a_look_without_a_reading_with_the_code_37() {
    let mut j = job(&format!(
        "align = on\nvision = pipeline\nsim_align_dx_mm = 40\n{GIVE_UP}"
    ));
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    let s = j.stats();
    assert_eq!((s.placed, s.deferred), (0, 6));
    assert_eq!(
        (s.align.camera_failures, s.align.rejected, s.align.corrected),
        (6, 0, 0)
    );
    let why = aside(&j);
    assert_eq!(why.len(), 6);
    assert!(
        why.iter().all(|w| w.starts_with("PHOENIX-PNP-037: ")),
        "{why:?}"
    );
    assert!(texts(&j)
        .iter()
        .any(|t| t == "the vision found no part on nozzle 1: PHOENIX-PNP-037"));
    // the picks were all good: the failures of the vision are not failures of the machine
    assert_eq!(s.pick_failures, 0);
}

#[test]
fn a_spot_out_of_the_range_of_area_is_the_code_38() {
    let mut j = job(&format!("{SEEN}{GIVE_UP}"));
    j.set_vision(VisionConfig {
        max_area_px: 100,
        ..VisionConfig::default()
    });
    j.run(None).unwrap();
    assert_eq!(j.stats().deferred, 6);
    assert!(aside(&j).iter().all(|w| w.starts_with("PHOENIX-PNP-038: ")));
    let mut small = job(&format!("{SEEN}{GIVE_UP}"));
    small.set_vision(VisionConfig {
        min_area_px: 3000,
        max_area_px: 4000,
        ..VisionConfig::default()
    });
    small.run(None).unwrap();
    assert!(aside(&small)
        .iter()
        .all(|w| w.starts_with("PHOENIX-PNP-037: ")));
}

#[test]
fn the_tolerance_is_judged_on_the_measured_distance() {
    // the part is 0.6 mm from the nozzle (12 pixels) and the tolerance is 0.5
    let mut far = job(&format!(
        "align = on\nvision = pipeline\nsim_align_dx_mm = 0.6\n{GIVE_UP}"
    ));
    far.run(None).unwrap();
    assert_eq!(far.stats().align.rejected, 6);
    assert!(aside(&far)
        .iter()
        .all(|w| w.starts_with("PHOENIX-PNP-035: ")));
    let mut near_enough = job("align = on\nvision = pipeline\nsim_align_dx_mm = 0.4\n");
    near_enough.run(None).unwrap();
    assert_eq!(near_enough.stats().align.rejected, 0);
}

#[test]
fn the_camera_that_fails_by_chance_still_does_with_the_vision_on() {
    let mut j = job(&format!("{SEEN}sim_camera_fail_rate = 1\n{GIVE_UP}"));
    j.run(None).unwrap();
    assert_eq!(j.stats().align.camera_failures, 6);
    assert!(aside(&j).iter().all(|w| w.starts_with("PHOENIX-PNP-036: ")));
}

// ---- the saved state

#[test]
fn a_saved_job_with_the_vision_resumes_anywhere_with_the_same_readings() {
    let cfg = format!(
        "{SEEN}align_passes = 2\nsim_camera_fail_rate = 0.3\nsim_seed = 5\nerror_policy = defer\npick_retries = 1\nfault_limit = 0\n"
    );
    let noisy = VisionConfig {
        sim_noise_levels: 120,
        ..VisionConfig::default()
    };
    let mut whole = job(&cfg);
    whole.set_vision(noisy.clone());
    whole.run(None).unwrap();
    assert!(whole.stats().align.corrected > 0);
    for stop in 0..=14 {
        let mut first = job(&cfg);
        first.set_vision(noisy.clone());
        first.run(Some(stop)).unwrap();
        let mut second = job(&cfg);
        second.set_vision(noisy.clone());
        second.restore_state(&first.save_state()).unwrap();
        second.resume();
        second.run(None).unwrap();
        assert_eq!(second.stats(), whole.stats(), "stop {stop}");
        assert_eq!(second.stats_json(), whole.stats_json(), "stop {stop}");
        assert_eq!(second.machine().position(), whole.machine().position());
    }
}

// ---- the command line

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

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eh3d_pnp_vision_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const TURNED: [&str; 14] = [
    "vision-test",
    "--size",
    "64x64",
    "--part",
    "4x2",
    "--mm-per-px",
    "0.1",
    "--dx",
    "0.5",
    "--dy",
    "-0.3",
    "--turn",
    "30",
    "--seed",
];

#[test]
fn vision_test_measures_a_part_and_prints_the_error() {
    let mut args = TURNED.to_vec();
    args.push("1");
    let o = run(&args);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    assert!(
        text.contains("picture 64x64 pixels, 0.100 mm per pixel, threshold 128"),
        "{text}"
    );
    assert!(
        text.contains("true:     dx 0.500 mm, dy -0.300 mm, turn 30.00 degrees"),
        "{text}"
    );
    let measured = text.lines().find(|l| l.starts_with("measured:")).unwrap();
    assert!(
        measured.contains("dx 0.500 mm, dy -0.300 mm, turn 29.7"),
        "{measured}"
    );
    assert!(
        measured.contains("spot of 798 pixels, rectangle of 41.2 x 21.3 pixels"),
        "{measured}"
    );
    let error = text.lines().find(|l| l.starts_with("error:")).unwrap();
    assert!(
        error.contains("dx +0.000 mm, dy +0.000 mm, turn -0.2"),
        "{error}"
    );
    assert!(error.ends_with("distance 0.000 mm"), "{error}");
}

#[test]
fn the_example_file_of_the_vision_gives_what_the_defaults_give() {
    let plain = run(&["vision-test", "--turn", "12", "--dx", "0.2"]);
    let file = run(&[
        "vision-test",
        "--vision",
        &example("vision.cfg"),
        "--turn",
        "12",
        "--dx",
        "0.2",
    ]);
    assert!(plain.status.success() && file.status.success());
    assert_eq!(out(&plain), out(&file));
    assert!(out(&plain).contains("picture 128x128 pixels, 0.050 mm per pixel"));
}

#[test]
fn vision_test_with_noise_is_the_same_for_the_same_seed() {
    let a = run(&[
        "vision-test",
        "--noise",
        "100",
        "--seed",
        "4",
        "--turn",
        "-20",
    ]);
    let b = run(&[
        "vision-test",
        "--noise",
        "100",
        "--seed",
        "4",
        "--turn",
        "-20",
    ]);
    assert!(a.status.success(), "{}", err(&a));
    assert_eq!(out(&a), out(&b));
}

#[test]
fn a_part_that_is_not_found_is_an_error_with_its_code_in_the_language_asked_for() {
    let o = run(&["vision-test", "--dx", "40"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("PHOENIX-PNP-037"), "{}", err(&o));
    let es = run(&["vision-test", "--dx", "40", "--lang", "es"]);
    assert!(
        err(&es).contains("La visión no encontró la pieza en la imagen"),
        "{}",
        err(&es)
    );
    let pt = run(&["vision-test", "--dx", "40", "--lang", "pt"]);
    assert!(
        err(&pt).contains("A visão não encontrou a peça na imagem"),
        "{}",
        err(&pt)
    );
    // a part that fills the picture is too large for the range of area
    let big = run(&["vision-test", "--part", "6x6"]);
    assert_eq!(big.status.code(), Some(1));
    assert!(err(&big).contains("PHOENIX-PNP-038"), "{}", err(&big));
}

#[test]
fn vision_test_refuses_what_it_cannot_draw() {
    let cases: [&[&str]; 14] = [
        &["vision-test", "--size", "10x10"],
        &["vision-test", "--size", "100"],
        &["vision-test", "--size", "2000x100"],
        &["vision-test", "--part", "0x1"],
        &["vision-test", "--part", "1x2"],
        &["vision-test", "--noise", "128"],
        &["vision-test", "--noise", "1.5"],
        &["vision-test", "--seed", "-1"],
        &["vision-test", "--mm-per-px", "0"],
        &["vision-test", "--turn", "abc"],
        &["vision-test", "--lang", "xx"],
        &["vision-test", "--oops"],
        &["vision-test", "extra"],
        &["vision-test", "--vision", "/no/such/vision.cfg"],
    ];
    for args in cases {
        let o = run(args);
        assert_eq!(o.status.code(), Some(1), "{args:?}");
        assert!(err(&o).starts_with("error: "), "{args:?}: {}", err(&o));
    }
}

#[test]
fn a_vision_file_with_a_mistake_names_the_code_and_the_line() {
    let dir = scratch("bad");
    let file = dir.join("vision.cfg");
    std::fs::write(&file, "# settings\nthreshold = 999\n").unwrap();
    let o = run(&["vision-test", "--vision", &file.to_string_lossy()]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("PHOENIX-PNP-006"), "{}", err(&o));
    assert!(err(&o).contains("line 2"), "{}", err(&o));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn simulate_measures_pictures_with_the_vision_settings_of_a_file() {
    let dir = scratch("simulate");
    let cfg = dir.join("machine.cfg");
    std::fs::write(
        &cfg,
        "align = on\nvision = pipeline\nsim_align_dx_mm = 0.3\nsim_align_dy_mm = 0.4\npick_tolerance_mm = 1\n",
    )
    .unwrap();
    let cfg = cfg.to_string_lossy().into_owned();
    let o = run(&[
        "simulate",
        &example("board.pos"),
        &example("feeders.csv"),
        "--config",
        &cfg,
        "--vision",
        &example("vision.cfg"),
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    let line = text.lines().find(|l| l.starts_with("align: ")).unwrap();
    assert!(
        line.starts_with("align: 6 parts corrected (mean "),
        "{line}"
    );
    assert!(line.ends_with("0 looks without a reading"), "{line}");
    // the range of area of a file that cannot hold the part: every part is put aside with the code 38
    let narrow = dir.join("narrow.cfg");
    std::fs::write(&narrow, "max_area_px = 60\n").unwrap();
    let cfg2 = dir.join("defer.cfg");
    std::fs::write(
        &cfg2,
        "align = on\nvision = pipeline\nerror_policy = defer\npick_retries = 0\nfault_limit = 0\n",
    )
    .unwrap();
    let o = run(&[
        "simulate",
        &example("board.pos"),
        &example("feeders.csv"),
        "--config",
        &cfg2.to_string_lossy(),
        "--vision",
        &narrow.to_string_lossy(),
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    assert!(
        text.lines()
            .any(|l| l.starts_with("put aside: ") && l.contains("PHOENIX-PNP-038")),
        "{text}"
    );
    assert!(text.contains("6 looks without a reading"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn simulate_refuses_a_bad_vision_file_and_never_writes_over_it() {
    let dir = scratch("refuse");
    let file = dir.join("vision.cfg");
    std::fs::write(&file, "min_area_px = 9\n").unwrap();
    let file = file.to_string_lossy().into_owned();
    let o = run(&[
        "simulate",
        &example("board.pos"),
        &example("feeders.csv"),
        "--vision",
        &file,
        "--report-csv",
        &file,
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("also an input"), "{}", err(&o));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "min_area_px = 9\n");
    std::fs::write(&file, "colour = red\n").unwrap();
    let o = run(&[
        "simulate",
        &example("board.pos"),
        &example("feeders.csv"),
        "--vision",
        &file,
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("PHOENIX-PNP-004"), "{}", err(&o));
    let _ = std::fs::remove_dir_all(&dir);
}
