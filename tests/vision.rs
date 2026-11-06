//! The vision pipeline on pictures made in memory: the numbers of the small pictures are worked out by hand, the turned rectangle was recalculated with a second program that does not share
//! any code with this one, and the failures, the settings and the simulated camera follow. None of it needs a screen or a camera.

use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::{Code, Lang};
use eh3d_phoenix_pnp::model::{Camera, Offset};
use eh3d_phoenix_pnp::sim::{Rng, SimCamera};
use eh3d_phoenix_pnp::vision::{
    default_file, measure_image, median3, min_area_rect, otsu_level, render, wrap, Axis, Filter,
    Image, Mask, ThresholdMode, VisionConfig, KEYS,
};
use std::collections::HashSet;

fn near(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

/// Settings for the small pictures: 0.1 mm per pixel, no filter, a fixed threshold.
fn small() -> VisionConfig {
    VisionConfig {
        mm_per_px: 0.1,
        filter: Filter::None,
        min_area_px: 5,
        max_area_px: 100,
        ..VisionConfig::default()
    }
}

/// A 16 x 16 picture of grey 20 with a part of grey 200 from `x0` up to `x1` and from `y0` up to `y1`.
fn block(x0: usize, y0: usize, x1: usize, y1: usize) -> Image {
    let mut img = Image::new(16, 16, 20);
    img.fill_rect(x0, y0, x1, y1, 200);
    img
}

fn code_of(text: &str) -> Code {
    VisionConfig::parse(text).unwrap_err().code
}

// ---- numbers worked out by hand

#[test]
fn a_block_of_pixels_gives_its_middle_its_sides_and_its_area() {
    // 8 pixels wide and 4 high, from column 6 and from row 2: it covers x from 6 to 14 and y from 2 to 6, so its middle is at 10, 4 and the middle of the 16 x 16 picture is at 8, 8
    let r = measure_image(&small(), &block(6, 2, 14, 6)).unwrap();
    assert_eq!(r.area_px, 32);
    assert_eq!(r.threshold, 128);
    assert!(near(r.centre_px.0, 10.0, 1e-9) && near(r.centre_px.1, 4.0, 1e-9));
    assert!(near(r.long_px, 8.0, 1e-9) && near(r.short_px, 4.0, 1e-9));
    // two pixels to the right of the axis, 0.1 mm each; four pixels above it: the first row is the top, so above is a positive dy
    assert!(near(r.offset.dx_mm, 0.2, 1e-9), "{}", r.offset.dx_mm);
    assert!(near(r.offset.dy_mm, 0.4, 1e-9), "{}", r.offset.dy_mm);
    assert!(near(r.offset.rotation_deg, 0.0, 1e-9));
}

#[test]
fn a_part_lying_up_and_down_is_turned_a_quarter_unless_the_long_side_is_expected_there() {
    // 4 wide and 8 high: the middle is at 9, 7, so 1 pixel to the right and 1 above the axis
    let img = block(7, 3, 11, 11);
    let r = measure_image(&small(), &img).unwrap();
    assert!(near(r.offset.dx_mm, 0.1, 1e-9) && near(r.offset.dy_mm, 0.1, 1e-9));
    assert!(near(r.long_px, 8.0, 1e-9) && near(r.short_px, 4.0, 1e-9));
    assert!(near(r.offset.rotation_deg, 90.0, 1e-9));
    let tall = VisionConfig {
        nominal_long_axis: Axis::Y,
        ..small()
    };
    let r = measure_image(&tall, &img).unwrap();
    assert!(near(r.offset.rotation_deg, 0.0, 1e-9));
}

#[test]
fn a_square_is_known_up_to_a_quarter_turn() {
    let r = measure_image(&small(), &block(4, 4, 10, 10)).unwrap();
    assert!(near(r.long_px, 6.0, 1e-9) && near(r.short_px, 6.0, 1e-9));
    assert!(near(r.offset.rotation_deg, 0.0, 1e-9));
    assert!(near(wrap(50.0, 90.0), -40.0, 1e-12));
    assert!(near(wrap(45.0, 90.0), 45.0, 1e-12));
    assert!(near(wrap(-45.0, 90.0), 45.0, 1e-12));
    assert!(near(wrap(-90.0, 180.0), 90.0, 1e-12));
    assert!(near(wrap(270.0, 180.0), 90.0, 1e-12));
}

#[test]
fn the_smallest_rectangle_of_a_set_of_points_is_found_by_its_hull() {
    // a rectangle of 6 x 2 and one more point inside it
    let points = [(0, 0), (6, 0), (6, 2), (0, 2), (3, 1)];
    let r = min_area_rect(&points).unwrap();
    assert!(near(r.long, 6.0, 1e-9) && near(r.short, 2.0, 1e-9));
    assert!(near(r.centre.0, 3.0, 1e-9) && near(r.centre.1, 1.0, 1e-9));
    // a diamond: its smallest rectangle lies along its sides, a square of side 5 (the diagonals are 10 and 10 but the bounding box along them is 10 x 10 = 100 against 5 x 5 x 2 = 50)
    let diamond = [(5, 0), (10, 5), (5, 10), (0, 5)];
    let d = min_area_rect(&diamond).unwrap();
    let side: f64 = 50.0_f64.sqrt();
    assert!(near(d.long, side, 1e-9) && near(d.short, side, 1e-9));
    // points on a line have no rectangle
    assert!(min_area_rect(&[(0, 0), (1, 1), (2, 2)]).is_none());
    assert!(min_area_rect(&[(0, 0), (1, 1)]).is_none());
}

// ---- the turned rectangle, recalculated with a second program

/// 64 x 64 pixels of 0.1 mm, a part of 4 x 2 mm turned 30 degrees counter-clockwise, 0.5 mm to the right of the axis and 0.3 mm below it.
fn turned_30() -> (VisionConfig, Offset) {
    let cfg = VisionConfig {
        mm_per_px: 0.1,
        image_width: 64,
        image_height: 64,
        sim_part_long_mm: 4.0,
        sim_part_short_mm: 2.0,
        max_area_px: 2000,
        ..VisionConfig::default()
    };
    let truth = Offset {
        dx_mm: 0.5,
        dy_mm: -0.3,
        rotation_deg: 30.0,
    };
    (cfg, truth)
}

// the program that recalculated it counts the pixels whose middle is inside the rectangle, takes the corners of those pixels and tries each side of their convex hull
// as the side of the rectangle of the smallest area; none of the pixels is closer than 0.003 pixels to an edge, so the count does not depend on the rounding of the machine
const AREA_30: usize = 798;
const LONG_30: f64 = 41.179_531_883_617_21;
const SHORT_30: f64 = 21.333_974_349_343_855;
const TURN_30: f64 = 29.744_881_296_942_22;

#[test]
fn a_part_turned_30_degrees_in_64_by_64_pixels_is_measured_as_the_second_program_says() {
    let (cfg, truth) = turned_30();
    let img = render(&cfg, truth, &mut Rng::new(1));
    let r = measure_image(&cfg, &img).unwrap();
    assert_eq!(r.area_px, AREA_30);
    assert!(near(r.long_px, LONG_30, 1e-6), "{}", r.long_px);
    assert!(near(r.short_px, SHORT_30, 1e-6), "{}", r.short_px);
    assert!(
        near(r.offset.rotation_deg, TURN_30, 1e-6),
        "{}",
        r.offset.rotation_deg
    );
    // the middle: 5 pixels to the right of the axis and 3 below it, so the middle of the rectangle is at 37, 35 in a picture whose axis is at 32, 32
    assert!(near(r.centre_px.0, 37.0, 1e-6), "{}", r.centre_px.0);
    assert!(near(r.centre_px.1, 35.0, 1e-6), "{}", r.centre_px.1);
    assert!(near(r.offset.dx_mm, 0.5, 1e-6) && near(r.offset.dy_mm, -0.3, 1e-6));
}

#[test]
fn the_error_of_the_measure_is_inside_what_one_pixel_allows() {
    // The edge of a part is a staircase of pixels, so the side of the smallest rectangle can lean by up to one pixel over the long side: atan(1 / 40) = 1.43 degrees for a
    // part 40 pixels long (the second program tried 120 places and turns and the worst was 1.40 degrees), and the middle can be off by up to half a pixel on each axis, 0.05 mm.
    let (cfg, base) = turned_30();
    let limit_deg: f64 = (1.0_f64 / 40.0).atan().to_degrees();
    let mut worst_deg: f64 = 0.0;
    let mut turns = vec![
        -85.0, -60.0, -45.0, -30.0, -10.0, 0.0, 10.0, 45.0, 60.0, 85.0,
    ];
    turns.push(base.rotation_deg);
    for turn in turns {
        for (dx, dy) in [(0.0, 0.0), (0.37, -0.21), (-0.44, 0.13)] {
            let truth = Offset {
                dx_mm: dx,
                dy_mm: dy,
                rotation_deg: turn,
            };
            let img = render(&cfg, truth, &mut Rng::new(7));
            let r = measure_image(&cfg, &img).unwrap();
            let miss: f64 = wrap(r.offset.rotation_deg - turn, 180.0).abs();
            worst_deg = worst_deg.max(miss);
            assert!(miss <= limit_deg + 0.1, "turn {turn}: {miss} degrees");
            assert!(
                near(r.offset.dx_mm, dx, 0.05),
                "turn {turn}: {}",
                r.offset.dx_mm
            );
            assert!(
                near(r.offset.dy_mm, dy, 0.05),
                "turn {turn}: {}",
                r.offset.dy_mm
            );
        }
    }
    assert!(worst_deg > 0.0, "the turned parts are not exact");
}

#[test]
fn a_noise_that_does_not_reach_the_threshold_changes_nothing() {
    // the background is 30 and the part 200, the threshold 128: a noise of up to 60 levels moves them to 90 and 140 at the most, on the right side of the threshold
    let (cfg, truth) = turned_30();
    let clean = measure_image(&cfg, &render(&cfg, truth, &mut Rng::new(3))).unwrap();
    let noisy_cfg = VisionConfig {
        sim_noise_levels: 60,
        ..cfg.clone()
    };
    for seed in [1, 2, 3] {
        let img = render(&noisy_cfg, truth, &mut Rng::new(seed));
        assert_eq!(
            measure_image(&noisy_cfg, &img).unwrap(),
            clean,
            "seed {seed}"
        );
    }
}

#[test]
fn a_noise_that_crosses_the_threshold_is_taken_out_by_the_filter_and_otsu() {
    let (cfg, truth) = turned_30();
    let hard = VisionConfig {
        sim_noise_levels: 100,
        filter: Filter::Median3,
        threshold_mode: ThresholdMode::Otsu,
        ..cfg
    };
    for seed in 1..=4 {
        let img = render(&hard, truth, &mut Rng::new(seed));
        let r = measure_image(&hard, &img).unwrap();
        let miss: f64 = wrap(r.offset.rotation_deg - 30.0, 180.0).abs();
        assert!(miss <= 2.0, "seed {seed}: {miss} degrees");
        assert!(
            near(r.offset.dx_mm, 0.5, 0.06),
            "seed {seed}: {}",
            r.offset.dx_mm
        );
        assert!(
            near(r.offset.dy_mm, -0.3, 0.06),
            "seed {seed}: {}",
            r.offset.dy_mm
        );
    }
}

#[test]
fn the_same_seed_gives_the_same_picture_and_another_seed_another() {
    let (cfg, truth) = turned_30();
    let noisy = VisionConfig {
        sim_noise_levels: 40,
        ..cfg
    };
    let a = render(&noisy, truth, &mut Rng::new(11));
    assert_eq!(a, render(&noisy, truth, &mut Rng::new(11)));
    assert_ne!(a, render(&noisy, truth, &mut Rng::new(12)));
    // the same part without noise is the same whatever the seed
    let clean = VisionConfig {
        sim_noise_levels: 0,
        ..noisy
    };
    assert_eq!(
        render(&clean, truth, &mut Rng::new(1)),
        render(&clean, truth, &mut Rng::new(2))
    );
}

#[test]
fn a_part_turned_a_quarter_looks_the_same_as_one_not_turned_only_for_a_square() {
    let cfg = VisionConfig {
        mm_per_px: 0.1,
        sim_part_long_mm: 3.0,
        sim_part_short_mm: 3.0,
        ..VisionConfig::default()
    };
    // a square of 30 x 30 pixels turned 50 degrees is a square turned -40
    let truth = Offset {
        rotation_deg: 50.0,
        ..Offset::default()
    };
    let r = measure_image(&cfg, &render(&cfg, truth, &mut Rng::new(1))).unwrap();
    let miss: f64 = wrap(r.offset.rotation_deg + 40.0, 90.0).abs();
    assert!(miss <= 2.5, "{}", r.offset.rotation_deg);
}

// ---- the steps one by one

#[test]
fn otsu_finds_the_lowest_level_that_separates_two_greys() {
    // a picture of two greys, 20 and 200: every level from 20 to 199 separates them equally well, the lowest wins
    let img = block(6, 2, 14, 6);
    assert_eq!(otsu_level(&img, &Mask::None), Some(20));
    // a picture of one grey has no level
    assert_eq!(otsu_level(&Image::new(8, 8, 90), &Mask::None), None);
    let mut cfg = small();
    cfg.threshold_mode = ThresholdMode::Otsu;
    let r = measure_image(&cfg, &img).unwrap();
    assert_eq!((r.threshold, r.area_px), (20, 32));
    let flat = measure_image(&cfg, &Image::new(16, 16, 90)).unwrap_err();
    assert_eq!(flat.code, Code::VisionNotFound);
}

#[test]
fn otsu_looks_only_inside_the_mask() {
    // a part of 200 on 20, and a corner of 255 that the mask leaves out: with it the level would separate 255 from the rest
    let mut img = block(6, 2, 14, 6);
    img.fill_rect(0, 12, 4, 16, 255);
    let mask = Mask::Rect {
        x0: 0,
        y0: 0,
        x1: 16,
        y1: 10,
    };
    assert_eq!(otsu_level(&img, &mask), Some(20));
}

#[test]
fn a_dark_part_on_a_light_background_is_found_the_same() {
    let mut img = Image::new(16, 16, 220);
    img.fill_rect(6, 2, 14, 6, 30);
    let cfg = VisionConfig {
        part_is_dark: true,
        ..small()
    };
    let r = measure_image(&cfg, &img).unwrap();
    assert_eq!(r.area_px, 32);
    assert!(near(r.offset.dx_mm, 0.2, 1e-9) && near(r.offset.dy_mm, 0.4, 1e-9));
    // looked for as a bright part, it is the background that is found, and it is too large to be a part
    let bright = measure_image(&small(), &img).unwrap_err();
    assert_eq!(bright.code, Code::VisionAreaOutOfRange);
}

#[test]
fn the_median_takes_out_a_lone_pixel_and_keeps_a_straight_edge() {
    let mut img = Image::new(9, 9, 50);
    img.set(4, 4, 255);
    img.set(0, 0, 255);
    assert_eq!(median3(&img), Image::new(9, 9, 50));
    let mut step = Image::new(8, 8, 0);
    step.fill_rect(4, 0, 8, 8, 200);
    assert_eq!(median3(&step), step);
    // a hole of one pixel inside the part is filled
    let mut part = block(4, 4, 12, 12);
    part.set(8, 8, 20);
    assert_eq!(median3(&part).get(8, 8), 200);
}

#[test]
fn the_filter_gives_a_speck_in_the_part_back_to_it() {
    let mut img = block(2, 2, 14, 14);
    img.set(8, 8, 20);
    let cfg = VisionConfig {
        max_area_px: 200,
        ..small()
    };
    let r = measure_image(&cfg, &img).unwrap();
    // without the filter the hole is not part of the spot (143 of the 144 pixels); with it the hole is filled but the four corners of the block are rounded off, because each corner has 4 of its 9 pixels in the part (144 - 4)
    assert_eq!(r.area_px, 143);
    let filtered = VisionConfig {
        filter: Filter::Median3,
        ..cfg
    };
    assert_eq!(measure_image(&filtered, &img).unwrap().area_px, 140);
}

#[test]
fn a_mask_leaves_out_what_is_outside_it() {
    let mut img = block(6, 2, 14, 6);
    img.fill_rect(1, 10, 6, 15, 200);
    let two = measure_image(&small(), &img).unwrap_err();
    assert_eq!(two.code, Code::VisionSeveralSpots);
    let top = VisionConfig {
        mask: Mask::Rect {
            x0: 0,
            y0: 0,
            x1: 16,
            y1: 8,
        },
        ..small()
    };
    assert_eq!(measure_image(&top, &img).unwrap().area_px, 32);
    // a circle of radius 6 around the middle of the part: the middle of the pixel in the corner of the other block is further
    let round = VisionConfig {
        mask: Mask::Circle {
            cx: 10.0,
            cy: 4.0,
            r: 6.0,
        },
        ..small()
    };
    assert_eq!(measure_image(&round, &img).unwrap().area_px, 32);
    // a mask that holds none of the part finds nothing
    let away = VisionConfig {
        mask: Mask::Rect {
            x0: 0,
            y0: 0,
            x1: 4,
            y1: 8,
        },
        ..small()
    };
    assert_eq!(
        measure_image(&away, &img).unwrap_err().code,
        Code::VisionNotFound
    );
}

// ---- the failures

#[test]
fn a_picture_with_no_part_gives_the_code_37() {
    let e = measure_image(&small(), &Image::new(16, 16, 20)).unwrap_err();
    assert_eq!(e.code, Code::VisionNotFound);
    assert_eq!(e.code.label(), "PHOENIX-PNP-037");
    assert!(
        e.detail.contains("no spot of at least 5 pixels"),
        "{}",
        e.detail
    );
    // a speck smaller than the least area is noise, not a part
    let mut speck = Image::new(16, 16, 20);
    speck.set(3, 3, 255);
    speck.set(4, 3, 255);
    let e = measure_image(&small(), &speck).unwrap_err();
    assert_eq!(e.code, Code::VisionNotFound);
    assert!(e.detail.contains("the largest has 2"), "{}", e.detail);
    let empty = Image {
        width: 0,
        height: 0,
        pixels: Vec::new(),
    };
    assert_eq!(
        measure_image(&small(), &empty).unwrap_err().code,
        Code::VisionNotFound
    );
}

#[test]
fn a_spot_too_large_is_the_code_38_and_the_limits_are_inclusive() {
    let big = block(2, 2, 14, 14);
    let e = measure_image(&small(), &big).unwrap_err();
    assert_eq!(e.code, Code::VisionAreaOutOfRange);
    assert_eq!(e.code.label(), "PHOENIX-PNP-038");
    assert!(e.detail.contains("144 pixels"), "{}", e.detail);
    let exact = VisionConfig {
        min_area_px: 144,
        max_area_px: 144,
        ..small()
    };
    assert_eq!(measure_image(&exact, &big).unwrap().area_px, 144);
    let one_less = VisionConfig {
        max_area_px: 143,
        ..exact
    };
    assert_eq!(
        measure_image(&one_less, &big).unwrap_err().code,
        Code::VisionAreaOutOfRange
    );
}

#[test]
fn two_spots_are_the_code_39_and_touching_corners_are_one() {
    let mut two = block(2, 2, 7, 7);
    two.fill_rect(9, 9, 14, 14, 200);
    let e = measure_image(&small(), &two).unwrap_err();
    assert_eq!(e.code, Code::VisionSeveralSpots);
    assert_eq!(e.code.label(), "PHOENIX-PNP-039");
    assert!(e.detail.starts_with("2 spots"), "{}", e.detail);
    // two blocks that touch by a corner are one spot (the pixels are joined along the diagonals too)
    let mut joined = block(2, 2, 7, 7);
    joined.fill_rect(7, 7, 12, 12, 200);
    let r = measure_image(&small(), &joined).unwrap();
    assert_eq!(r.area_px, 50);
}

#[test]
fn the_codes_37_to_39_have_a_meaning_in_every_language() {
    for (code, number) in [
        (Code::VisionNotFound, "PHOENIX-PNP-037"),
        (Code::VisionAreaOutOfRange, "PHOENIX-PNP-038"),
        (Code::VisionSeveralSpots, "PHOENIX-PNP-039"),
    ] {
        assert_eq!(code.label(), number);
        let mut seen = HashSet::new();
        for lang in Lang::ALL {
            let m = code.message(lang);
            assert!(!m.is_empty());
            seen.insert(m);
        }
        assert_eq!(seen.len(), 8, "{number}");
    }
    assert!(Code::ALL.len() >= 39);
    assert_eq!(
        Code::VisionNotFound.message(Lang::Pt),
        "A visão não encontrou a peça na imagem"
    );
    assert_eq!(
        Code::VisionSeveralSpots.message(Lang::Pt),
        "A visão encontrou mais de uma mancha que pode ser a peça"
    );
    assert!(Code::VisionAreaOutOfRange
        .message(Lang::Es)
        .contains("área"));
}

// ---- the settings file

#[test]
fn the_default_file_is_the_default_and_the_example_is_the_default_file() {
    assert_eq!(
        VisionConfig::parse(default_file()).unwrap(),
        VisionConfig::default()
    );
    assert_eq!(VisionConfig::parse("").unwrap(), VisionConfig::default());
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/vision.cfg");
    let example = std::fs::read_to_string(path).unwrap().replace("\r\n", "\n");
    assert_eq!(example, default_file());
}

#[test]
fn the_default_file_names_every_key_and_nothing_else() {
    let mut in_file: Vec<&str> = default_file()
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, _)| k.trim())
        .collect();
    let mut known: Vec<&str> = KEYS.to_vec();
    in_file.sort_unstable();
    known.sort_unstable();
    known.dedup();
    assert_eq!(known.len(), KEYS.len(), "a key is listed twice");
    assert_eq!(in_file, known);
}

#[test]
fn a_file_sets_every_step() {
    let text = "threshold_mode = otsu\nthreshold = 99\npart_is_dark = yes\nfilter = none\nfilter_passes = 3\nmask = circle 64 64 30\nmin_area_px = 10\nmax_area_px = 900\nnominal_long_axis = Y\nsquare_ratio = 1.5\nmm_per_px = 0.02\nimage_width = 200\nimage_height = 100\nsim_part_long_mm = 3\nsim_part_short_mm = 1\nsim_background = 200\nsim_foreground = 20\nsim_noise_levels = 15\n";
    let c = VisionConfig::parse(text).unwrap();
    assert_eq!(c.threshold_mode, ThresholdMode::Otsu);
    assert_eq!((c.threshold, c.part_is_dark), (99, true));
    assert_eq!((c.filter, c.filter_passes), (Filter::None, 3));
    assert_eq!(
        c.mask,
        Mask::Circle {
            cx: 64.0,
            cy: 64.0,
            r: 30.0
        }
    );
    assert_eq!((c.min_area_px, c.max_area_px), (10, 900));
    assert_eq!(c.nominal_long_axis, Axis::Y);
    assert!(near(c.square_ratio, 1.5, 1e-12) && near(c.mm_per_px, 0.02, 1e-12));
    assert_eq!((c.image_width, c.image_height), (200, 100));
    assert_eq!(
        (c.sim_background, c.sim_foreground, c.sim_noise_levels),
        (200, 20, 15)
    );
    let rect = VisionConfig::parse("mask = rect 1 2 30 40").unwrap();
    assert_eq!(
        rect.mask,
        Mask::Rect {
            x0: 1,
            y0: 2,
            x1: 30,
            y1: 40
        }
    );
    assert_eq!(VisionConfig::parse("mask = NONE").unwrap().mask, Mask::None);
}

#[test]
fn a_bad_file_is_refused_with_its_code_and_its_line() {
    assert_eq!(code_of("nonsense"), Code::ConfigSyntax);
    assert_eq!(code_of("colour = red"), Code::ConfigUnknownKey);
    assert_eq!(
        code_of("threshold = 1\nthreshold = 2"),
        Code::ConfigDuplicateKey
    );
    assert_eq!(code_of("threshold = 256"), Code::ConfigOutOfRange);
    assert_eq!(code_of("threshold = -1"), Code::ConfigOutOfRange);
    assert_eq!(code_of("threshold = 1.5"), Code::ConfigBadValue);
    assert_eq!(code_of("mm_per_px = 0"), Code::ConfigOutOfRange);
    assert_eq!(code_of("mm_per_px = NaN"), Code::ConfigBadValue);
    assert_eq!(code_of("filter_passes = 6"), Code::ConfigOutOfRange);
    assert_eq!(code_of("filter = gauss"), Code::ConfigBadValue);
    assert_eq!(code_of("threshold_mode = adaptive"), Code::ConfigBadValue);
    assert_eq!(code_of("part_is_dark = maybe"), Code::ConfigBadValue);
    assert_eq!(code_of("nominal_long_axis = z"), Code::ConfigBadValue);
    assert_eq!(code_of("square_ratio = 0.9"), Code::ConfigOutOfRange);
    assert_eq!(code_of("image_width = 15"), Code::ConfigOutOfRange);
    assert_eq!(code_of("image_height = 1025"), Code::ConfigOutOfRange);
    assert_eq!(code_of("sim_noise_levels = 128"), Code::ConfigOutOfRange);
    assert_eq!(code_of("sim_part_long_mm = 51"), Code::ConfigOutOfRange);
    assert_eq!(code_of("schema_version = 2"), Code::ConfigSchemaNewer);
    for mask in [
        "mask = rect 1 2 3",
        "mask = rect 5 2 3 9",
        "mask = rect 1 2 3 2",
        "mask = rect -1 2 3 4",
        "mask = rect 1.5 2 3 4",
        "mask = circle 5 5",
        "mask = circle 5 5 0.2",
        "mask = circle a b c",
        "mask = none 1",
        "mask = ellipse 1 2 3 4",
        "mask =",
    ] {
        assert_eq!(code_of(mask), Code::ConfigBadValue, "{mask}");
    }
    let e = VisionConfig::parse("# a comment\n\nthreshold = 300\n").unwrap_err();
    assert!(e.detail.contains("line 3"), "{}", e.detail);
    let long = format!("threshold = {}", "9".repeat(2000));
    assert_eq!(code_of(&long), Code::ConfigSyntax);
    assert_eq!(code_of(&"#".repeat(70_000)), Code::FileTooBig);
}

#[test]
fn settings_that_do_not_fit_together_are_refused() {
    assert_eq!(
        code_of("min_area_px = 500\nmax_area_px = 100"),
        Code::UnsafeSetting
    );
    assert_eq!(
        code_of("sim_part_long_mm = 1\nsim_part_short_mm = 2"),
        Code::UnsafeSetting
    );
    assert_eq!(
        code_of("sim_background = 90\nsim_foreground = 90"),
        Code::UnsafeSetting
    );
    // the least area may be equal to the largest
    assert!(VisionConfig::parse("min_area_px = 100\nmax_area_px = 100").is_ok());
}

// ---- the simulated camera

fn pipeline_camera(extra: &str) -> SimCamera {
    let cfg = MachineConfig::parse(&format!("vision = pipeline\n{extra}")).unwrap();
    SimCamera::from_config(&cfg)
}

#[test]
fn the_machine_setting_chooses_where_the_camera_reads_from() {
    assert!(!SimCamera::from_config(&MachineConfig::default()).uses_vision());
    assert!(pipeline_camera("").uses_vision());
    assert_eq!(
        MachineConfig::parse("vision = OFF").unwrap().vision.name(),
        "off"
    );
    assert_eq!(
        MachineConfig::parse("vision = pipeline")
            .unwrap()
            .vision
            .name(),
        "pipeline"
    );
    assert_eq!(
        MachineConfig::parse("vision = laser").unwrap_err().code,
        Code::ConfigBadValue
    );
    // the settings of the pipeline are given only to a camera that uses it
    let mut direct = SimCamera::from_config(&MachineConfig::default());
    direct.set_vision(VisionConfig::default());
    assert!(!direct.uses_vision());
}

#[test]
fn the_camera_that_measures_a_picture_finds_the_offset_and_the_correction_takes_it_away() {
    let mut camera =
        pipeline_camera("sim_align_dx_mm = 0.3\nsim_align_dy_mm = -0.2\nsim_align_deg = 5\n");
    let seen = camera.measure(0).unwrap();
    // 0.05 mm per pixel: half a pixel is 0.025 mm, and the turn of a part 32 pixels long is known to atan(1 / 32) = 1.8 degrees
    assert!(near(seen.dx_mm, 0.3, 0.03), "{}", seen.dx_mm);
    assert!(near(seen.dy_mm, -0.2, 0.03), "{}", seen.dy_mm);
    assert!(near(seen.rotation_deg, 5.0, 1.9), "{}", seen.rotation_deg);
    camera.apply(seen);
    let again = camera.measure(0).unwrap();
    assert!(
        again.dx_mm.abs() <= 0.05 && again.dy_mm.abs() <= 0.05,
        "{again:?}"
    );
    assert!(again.rotation_deg.abs() <= 3.7, "{again:?}");
    // a new part is under the camera: it is seen where the first one was
    camera.clear();
    let fresh = camera.measure(0).unwrap();
    assert!(near(fresh.dx_mm, 0.3, 0.03));
}

#[test]
fn a_camera_that_sees_nothing_gives_the_codes_of_the_vision() {
    // a part 0.3 mm to the right is seen; one 40 mm away is out of the picture (6.4 mm wide)
    let mut far = pipeline_camera("sim_align_dx_mm = 40\n");
    assert_eq!(far.measure(0).unwrap_err().code, Code::VisionNotFound);
    // a part that is almost all out of the picture leaves a spot smaller than the least area: it is noise
    let mut edge = pipeline_camera("sim_align_dx_mm = 3.9\n");
    assert_eq!(edge.measure(0).unwrap_err().code, Code::VisionNotFound);
    // the chance of failing and the failures a test chooses still give no reading, and they are not the failures of the vision
    let mut chance = pipeline_camera("sim_camera_fail_rate = 1\n");
    assert_eq!(chance.measure(0).unwrap_err().code, Code::SimFault);
    let mut forced = pipeline_camera("");
    forced.force_fail(2);
    assert!(forced.measure(0).is_ok());
    assert_eq!(forced.measure(0).unwrap_err().code, Code::SimFault);
    assert!(forced.measure(0).is_ok());
}

#[test]
fn the_picture_of_each_look_comes_from_the_seed_and_the_number_of_the_look() {
    let noisy = VisionConfig {
        sim_noise_levels: 100,
        filter: Filter::None,
        ..VisionConfig::default()
    };
    let (mut a, mut b) = (
        pipeline_camera(
            "sim_seed = 5
",
        ),
        pipeline_camera(
            "sim_seed = 5
",
        ),
    );
    a.set_vision(noisy.clone());
    b.set_vision(noisy.clone());
    for _ in 0..3 {
        assert_eq!(a.measure(0).ok(), b.measure(0).ok());
    }
    // another seed draws other pictures
    let mut c = pipeline_camera(
        "sim_seed = 6
",
    );
    c.set_vision(noisy);
    let mut d = pipeline_camera(
        "sim_seed = 5
",
    );
    d.set_vision(VisionConfig {
        sim_noise_levels: 100,
        filter: Filter::None,
        ..VisionConfig::default()
    });
    let same = (0..3).all(|_| c.measure(0).ok() == d.measure(0).ok());
    assert!(!same);
}
