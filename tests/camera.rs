//! The model of the camera: places into pixels and pixels into places, the turn and the mirror of the picture, the lens, the settings of the camera and the heads' cameras, and the
//! simulated camera that reads a part through the model, with and without a picture.

use eh3d_phoenix_pnp::camera::CameraModel;
use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::{HeadSpec, MachineConfig, KEYS};
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::job::Job;
use eh3d_phoenix_pnp::model::Camera;
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use eh3d_phoenix_pnp::sim::SimCamera;

const EXAMPLE: &str = include_str!("../examples/machine.cfg");

fn near(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

fn code_of(text: &str) -> Code {
    MachineConfig::parse(text).unwrap_err().code
}

// ---- the places and the pixels

#[test]
fn the_default_model_changes_nothing() {
    let m = CameraModel::default();
    assert!(m.is_default());
    assert_eq!(m.read((1.25, -3.5), 0.05), (1.25, -3.5));
    assert_eq!(m.to_machine((10.0, 5.0), 0.05), (0.5, 0.25));
    assert_eq!(m.units(0.05), 0.05);
    assert_eq!(m.angle_to_image(33.0), 33.0);
    assert_eq!(m.angle_to_machine(-33.0), -33.0);
    // a size of its own wins over the one of the pipeline
    let own = CameraModel {
        units_per_px: 0.02,
        ..m
    };
    assert_eq!(own.units(0.05), 0.02);
    assert!(!own.is_default());
}

#[test]
fn a_turned_picture_with_an_offset_gives_the_place_worked_out_by_hand() {
    // a pixel 10 to the right and 5 above the middle is 0.5 and 0.25 mm; turned 90 degrees that is (-0.25, 0.5); the axis of the nozzle is 1 and 2 mm from the middle
    let m = CameraModel {
        rotation_deg: 90.0,
        offset_x_mm: 1.0,
        offset_y_mm: 2.0,
        ..CameraModel::default()
    };
    let (x, y) = m.to_machine((10.0, 5.0), 0.05);
    assert!(near(x, -1.25, 1e-12) && near(y, -1.5, 1e-12), "{x} {y}");
    let (px, py) = m.to_pixels((-1.25, -1.5), 0.05);
    assert!(near(px, 10.0, 1e-9) && near(py, 5.0, 1e-9), "{px} {py}");
}

#[test]
fn a_mirrored_picture_changes_the_sign_of_its_axis() {
    let fx = CameraModel {
        flip_x: true,
        ..CameraModel::default()
    };
    assert_eq!(fx.to_machine((10.0, 5.0), 0.05), (-0.5, 0.25));
    let fy = CameraModel {
        flip_y: true,
        ..CameraModel::default()
    };
    assert_eq!(fy.to_machine((10.0, 5.0), 0.05), (0.5, -0.25));
    let both = CameraModel {
        flip_x: true,
        flip_y: true,
        ..CameraModel::default()
    };
    assert_eq!(both.to_machine((10.0, 5.0), 0.05), (-0.5, -0.25));
    // the turn of a part follows: 30 degrees is 150 in a picture mirrored left to right, -30 upside down and -150 (a half turn from it) in both
    assert_eq!(fx.angle_to_image(30.0), 150.0);
    assert_eq!(fy.angle_to_image(30.0), -30.0);
    assert_eq!(both.angle_to_image(30.0), -150.0);
}

#[test]
fn the_lens_scales_and_bends_what_the_software_reads() {
    // the true pixel is 2 % larger than the software believes: every distance is read 1.02 times too small
    let scaled = CameraModel {
        lens_scale: 1.02,
        ..CameraModel::default()
    };
    let (x, y) = scaled.read((1.0, 2.0), 0.05);
    assert!(near(x, 0.980_392_156_862_745_1, 1e-12), "{x}");
    assert!(near(y, 1.960_784_313_725_490_2, 1e-12), "{y}");
    // a point 5 mm from the middle is drawn 1 + 0.001 * 25 = 1.025 times further out
    let bent = CameraModel {
        lens_k1: 0.001,
        ..CameraModel::default()
    };
    let (x, y) = bent.read((3.0, 4.0), 0.05);
    assert!(near(x, 3.075, 1e-12) && near(y, 4.1, 1e-12), "{x} {y}");
    // the size of the pixel cancels: the same place is read the same whatever the units are
    assert_eq!(scaled.read((1.0, 2.0), 0.05), scaled.read((1.0, 2.0), 0.2));
    // the centre of the picture stays where it is
    assert_eq!(bent.read((0.0, 0.0), 0.05), (0.0, 0.0));
}

#[test]
fn a_place_goes_through_a_perfect_lens_and_comes_back_for_any_turn_and_mirror() {
    let mut state = 7u64;
    let mut next = move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
    };
    for i in 0..200 {
        let m = CameraModel {
            units_per_px: 0.01 + 0.04 * next().abs(),
            offset_x_mm: 5.0 * next(),
            offset_y_mm: 5.0 * next(),
            rotation_deg: 180.0 * next(),
            flip_x: i % 2 == 0,
            flip_y: i % 3 == 0,
            ..CameraModel::default()
        };
        let place = (10.0 * next(), 10.0 * next());
        let back = m.read(place, m.units(0.05));
        assert!(
            near(back.0, place.0, 1e-9) && near(back.1, place.1, 1e-9),
            "{i}: {place:?} {back:?}"
        );
        let turn = 170.0 * next();
        let there = m.angle_to_machine(m.angle_to_image(turn));
        assert!(near(there, turn, 1e-9), "{i}: {turn} {there}");
    }
}

#[test]
fn the_turn_of_a_part_is_measured_in_the_frame_of_the_picture() {
    let m = CameraModel {
        rotation_deg: 90.0,
        ..CameraModel::default()
    };
    assert_eq!(m.angle_to_image(30.0), -60.0);
    assert_eq!(m.angle_to_machine(-60.0), 30.0);
    let mirrored = CameraModel { flip_x: true, ..m };
    assert_eq!(mirrored.angle_to_image(30.0), 240.0);
    assert_eq!(mirrored.angle_to_machine(240.0), 30.0);
}

// ---- the settings

#[test]
fn the_settings_of_the_camera_have_their_defaults_and_the_example_is_the_default() {
    let d = MachineConfig::parse("").unwrap();
    assert!(d.camera.is_default());
    assert!(d.heads.is_empty() && d.tip_rules.is_empty());
    let e = MachineConfig::parse(EXAMPLE).unwrap();
    assert_eq!(e.camera, d.camera);
    // the example only adds the head that a machine has when it declares none and a rule that matches no package of the examples
    assert_eq!(e.heads.len(), 1);
    assert_eq!(
        e.heads[0],
        HeadSpec {
            name: "main".to_string(),
            ..HeadSpec::implicit(2)
        }
    );
    assert_eq!(e.heads[0].nozzles, e.nozzles);
    assert_eq!(e.tip_rules.len(), 1);
    assert_eq!(e.tip_choice, d.tip_choice);
    for key in [
        "camera_units_per_px",
        "camera_offset_x_mm",
        "camera_offset_y_mm",
        "camera_rotation_deg",
        "camera_flip_x",
        "camera_flip_y",
        "camera_lens_scale",
        "camera_lens_k1",
        "head",
        "tip_compat",
        "tip_compat_mode",
    ] {
        assert!(KEYS.contains(&key), "{key}");
    }
}

#[test]
fn the_camera_settings_have_strict_ranges() {
    let c = MachineConfig::parse(
        "camera_units_per_px = 0.02\ncamera_offset_x_mm = 1.5\ncamera_offset_y_mm = -2\ncamera_rotation_deg = 90\ncamera_flip_x = on\ncamera_flip_y = no\ncamera_lens_scale = 1.1\ncamera_lens_k1 = -0.002",
    )
    .unwrap();
    let m = c.camera;
    assert!(near(m.units_per_px, 0.02, 1e-12) && near(m.offset_x_mm, 1.5, 1e-12));
    assert!(near(m.offset_y_mm, -2.0, 1e-12) && near(m.rotation_deg, 90.0, 1e-12));
    assert!(m.flip_x && !m.flip_y);
    assert!(near(m.lens_scale, 1.1, 1e-12) && near(m.lens_k1, -0.002, 1e-12));
    for bad in [
        "camera_units_per_px = 0.0005",
        "camera_units_per_px = 11",
        "camera_units_per_px = -1",
        "camera_offset_x_mm = 501",
        "camera_offset_y_mm = -501",
        "camera_rotation_deg = 181",
        "camera_lens_scale = 0.4",
        "camera_lens_scale = 2.1",
        "camera_lens_k1 = 0.02",
        "camera_lens_k1 = -0.02",
    ] {
        assert_eq!(code_of(bad), Code::ConfigOutOfRange, "{bad}");
    }
    for bad in [
        "camera_flip_x = maybe",
        "camera_lens_scale = fast",
        "camera_rotation_deg = NaN",
    ] {
        assert_eq!(code_of(bad), Code::ConfigBadValue, "{bad}");
    }
    assert_eq!(
        code_of("camera_flip_x = on\ncamera_flip_x = off"),
        Code::ConfigDuplicateKey
    );
}

#[test]
fn the_camera_of_a_head_is_the_camera_of_the_configuration_with_what_the_head_says() {
    let c = MachineConfig::parse(
        "camera_units_per_px = 0.03\ncamera_rotation_deg = 10\ncamera_flip_y = on\ncamera_lens_scale = 1.1\nhead = a : 2\nhead = b : 1 : 0 : 0 : 3 : -4 : 45 : 0.04 : 1.2\nhead = c : 1 : 5 : 6 : : 7",
    )
    .unwrap();
    let base = c.camera;
    // a head with nothing about its camera has the camera of the configuration
    assert_eq!(c.heads[0].camera(&base), base);
    let b = c.heads[1].camera(&base);
    assert_eq!(
        (b.offset_x_mm, b.offset_y_mm, b.rotation_deg),
        (3.0, -4.0, 45.0)
    );
    assert!(near(b.units_per_px, 0.04, 1e-12) && near(b.lens_scale, 1.2, 1e-12));
    assert!(b.flip_y && !b.flip_x);
    // an empty field keeps the camera's value, a filled one after it is used
    let cc = c.heads[2].camera(&base);
    assert_eq!(
        (cc.offset_x_mm, cc.offset_y_mm, cc.rotation_deg),
        (0.0, 7.0, 10.0)
    );
    assert_eq!((c.heads[2].offset_x_mm, c.heads[2].offset_y_mm), (5.0, 6.0));
}

// ---- the simulated camera reads through the model

fn reading(cfg: &str) -> (f64, f64, f64) {
    let cfg = MachineConfig::parse(cfg).unwrap();
    let mut camera = SimCamera::from_config(&cfg);
    let o = camera.measure(0).unwrap();
    (o.dx_mm, o.dy_mm, o.rotation_deg)
}

#[test]
fn without_a_picture_the_camera_reads_the_part_through_its_lens() {
    let plain = reading("sim_align_dx_mm = 1\nsim_align_dy_mm = 2\nsim_align_deg = 7");
    assert_eq!(plain, (1.0, 2.0, 7.0));
    let (x, y, r) = reading(
        "sim_align_dx_mm = 1\nsim_align_dy_mm = 2\nsim_align_deg = 7\ncamera_lens_scale = 1.25",
    );
    assert!(near(x, 0.8, 1e-12) && near(y, 1.6, 1e-12) && near(r, 7.0, 1e-12));
    // a camera that is only turned, mirrored and offset reads the part where it is
    let (x, y, r) = reading(
        "sim_align_dx_mm = 1\nsim_align_dy_mm = 2\nsim_align_deg = 7\ncamera_rotation_deg = 90\ncamera_flip_x = on\ncamera_offset_x_mm = 3\ncamera_offset_y_mm = -1",
    );
    assert!(near(x, 1.0, 1e-9) && near(y, 2.0, 1e-9) && near(r, 7.0, 1e-9));
}

#[test]
fn with_a_picture_a_turned_or_mirrored_camera_still_finds_the_part_where_it_is() {
    let truth =
        "vision = pipeline\nsim_align_dx_mm = 0.3\nsim_align_dy_mm = -0.2\nsim_align_deg = 10\n";
    // 0.05 mm per pixel: the middle is found to half a pixel, the turn of a part 32 pixels long to 1.8 degrees
    for extra in [
        "",
        "camera_rotation_deg = 90\n",
        "camera_rotation_deg = -90\n",
        "camera_rotation_deg = 180\n",
        "camera_rotation_deg = 37\n",
        "camera_flip_x = on\n",
        "camera_flip_y = on\n",
        "camera_flip_x = on\ncamera_flip_y = on\ncamera_rotation_deg = 90\n",
        "camera_offset_x_mm = 0.5\ncamera_offset_y_mm = -0.4\ncamera_rotation_deg = 90\n",
    ] {
        let (x, y, r) = reading(&format!("{truth}{extra}"));
        assert!(
            near(x, 0.3, 0.04) && near(y, -0.2, 0.04) && near(r, 10.0, 1.9),
            "{extra:?}: {x} {y} {r}"
        );
    }
}

#[test]
fn a_lens_that_enlarges_the_picture_shows_in_the_readings_and_in_the_size_of_the_part() {
    // a true pixel 10 % larger than believed: the part is read 1.1 times too close to the middle
    let (x, y, _) = reading(
        "vision = pipeline\nsim_align_dx_mm = 0.88\nsim_align_dy_mm = -0.44\ncamera_lens_scale = 1.1",
    );
    assert!(near(x, 0.8, 0.04) && near(y, -0.4, 0.04), "{x} {y}");
    // a pixel five times larger than the pipeline is set for draws the part with a fifth of its pixels: the spot is too small to be the part
    let cfg = MachineConfig::parse("vision = pipeline\ncamera_units_per_px = 0.25").unwrap();
    let e = SimCamera::from_config(&cfg).measure(0).unwrap_err();
    assert!(
        matches!(e.code, Code::VisionNotFound | Code::VisionAreaOutOfRange),
        "{e}"
    );
    // a pixel as large as the pipeline's, said in the model, changes nothing
    let same = reading("vision = pipeline\ncamera_units_per_px = 0.05\nsim_align_dx_mm = 0.3");
    let plain = reading("vision = pipeline\nsim_align_dx_mm = 0.3");
    assert_eq!(same, plain);
}

#[test]
fn the_camera_of_a_head_reads_with_its_own_model() {
    let cfg = MachineConfig::parse(
        "sim_align_dx_mm = 1\nsim_align_dy_mm = 2\nhead = a : 1\nhead = b : 1 : 0 : 0 : : : : : 1.25",
    )
    .unwrap();
    let mut a = SimCamera::for_head(&cfg, &cfg.heads[0]);
    let mut b = SimCamera::for_head(&cfg, &cfg.heads[1]);
    let (oa, ob) = (a.measure(0).unwrap(), b.measure(0).unwrap());
    assert_eq!((oa.dx_mm, oa.dy_mm), (1.0, 2.0));
    assert!(near(ob.dx_mm, 0.8, 1e-12) && near(ob.dy_mm, 1.6, 1e-12));
    assert!(a.model().is_default() && !b.model().is_default());
}

// ---- the job that looks through the model

const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nC1,100n,C,10,20,0,top\n";
const FEEDERS: &str = "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\n";

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

fn looks(j: &Job) -> Vec<String> {
    j.machine()
        .events
        .iter()
        .map(|e| e.text.clone())
        .filter(|t| t.starts_with("look "))
        .collect()
}

#[test]
fn a_job_without_the_model_is_the_job_it_was_and_the_default_model_changes_nothing() {
    let base = "align = on\nsim_align_dx_mm = 0.4\nsim_align_dy_mm = -0.1\n";
    let mut plain = job(base);
    let mut explicit = job(&format!(
        "{base}camera_units_per_px = 0\ncamera_offset_x_mm = 0\ncamera_rotation_deg = 0\ncamera_flip_x = off\ncamera_lens_scale = 1\ncamera_lens_k1 = 0\n"
    ));
    plain.run(None).unwrap();
    explicit.run(None).unwrap();
    assert_eq!(plain.save_state(), explicit.save_state());
    assert_eq!(plain.stats_csv(), explicit.stats_csv());
}

#[test]
fn a_lens_that_reads_short_is_corrected_in_steps_by_the_passes_of_the_align_step() {
    // the camera reads a part 1 mm off as 0.8 mm: the first correction leaves 0.2 mm, which the second look reads as 0.16 mm
    let mut j =
        job("align = on\nalign_passes = 2\npick_tolerance_mm = 5\nsim_align_dx_mm = 1\ncamera_lens_scale = 1.25\n");
    j.run(None).unwrap();
    let seen = looks(&j);
    assert!(
        seen.iter()
            .any(|t| t == "look 1 at nozzle 1: X 0.800 Y 0.000 mm, turned 0.000 degrees"),
        "{seen:?}"
    );
    assert!(
        seen.iter()
            .any(|t| t == "look 2 at nozzle 1: X 0.160 Y 0.000 mm, turned 0.000 degrees"),
        "{seen:?}"
    );
}
