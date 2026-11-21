//! A feeder that drags the tape by its sprocket holes: the shortest stroke is a whole number of holes and of pockets, a stroke serves as many picks as pockets it brings, the picks of a stroke
//! lie in turn at the place and further along, a pitch that does not fit in a few picks is refused, and the numbers hold for pitches from a fixed seed. The command ends with 2 when a strip
//! cannot be dragged.

use eh3d_phoenix_pnp::drag::{csv, drag_of, plan};
use eh3d_phoenix_pnp::feeders::Feeder;
use eh3d_phoenix_pnp::feeding::{from_csv, FeederKind, FeederSpec};
use std::process::{Command, Output};

const FEEDERS: &str = include_str!("../examples/feeders_drag.csv");

fn strip(id: &str, pitch: f64, rotation: f64, count: u32) -> FeederSpec {
    let mut f = FeederSpec::from_basic(Feeder {
        id: id.into(),
        value: "10k".into(),
        package: "R_0402".into(),
        x: 100.0,
        y: 50.0,
        count,
    });
    f.kind = FeederKind::Strip { pitch, rotation };
    f
}

#[test]
fn a_pitch_of_2_mm_serves_two_picks_a_stroke_and_one_of_4_serves_one() {
    let two = drag_of(&strip("A", 2.0, 0.0, 100), 2.0, 0.0, 4.0, 4).unwrap();
    assert_eq!(
        (two.stroke_mm, two.holes_per_stroke, two.picks_per_stroke),
        (4.0, 1, 2)
    );
    assert_eq!(two.strokes, 50);
    let four = drag_of(&strip("B", 4.0, 0.0, 100), 4.0, 0.0, 4.0, 4).unwrap();
    assert_eq!((four.picks_per_stroke, four.strokes), (1, 100));
    // 3 mm: the first stroke that is whole holes and whole pockets is 12 mm, four picks
    let three = drag_of(&strip("C", 3.0, 0.0, 10), 3.0, 0.0, 4.0, 4).unwrap();
    assert_eq!(
        (
            three.stroke_mm,
            three.holes_per_stroke,
            three.picks_per_stroke
        ),
        (12.0, 3, 4)
    );
    assert_eq!(three.strokes, 3);
    // 8 mm pockets: one stroke is two holes
    let eight = drag_of(&strip("D", 8.0, 0.0, 7), 8.0, 0.0, 4.0, 4).unwrap();
    assert_eq!(
        (
            eight.stroke_mm,
            eight.holes_per_stroke,
            eight.picks_per_stroke
        ),
        (8.0, 2, 1)
    );
}

#[test]
fn the_picks_of_a_stroke_lie_in_turn_at_the_place_and_further_along_the_tape() {
    let d = drag_of(&strip("A", 2.0, 90.0, 10), 2.0, 90.0, 4.0, 4).unwrap();
    let origin = (100.0, 50.0);
    let near = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9;
    assert!(near(d.pick_point(origin, 0), (100.0, 50.0)));
    assert!(near(d.pick_point(origin, 1), (100.0, 52.0)));
    // after a stroke they start again at the place
    assert!(near(d.pick_point(origin, 2), (100.0, 50.0)));
    assert!(near(d.pick_point(origin, 3), (100.0, 52.0)));
    // the first stroke is made before the first pick
    assert_eq!(
        (
            d.strokes_after(0),
            d.strokes_after(1),
            d.strokes_after(2),
            d.strokes_after(3)
        ),
        (0, 1, 1, 2)
    );
}

#[test]
fn a_pitch_that_does_not_fit_in_a_few_picks_or_is_not_a_distance_is_refused() {
    // 3.5 mm against holes 4 mm apart needs 28 mm and eight picks
    let why = drag_of(&strip("A", 3.5, 0.0, 10), 3.5, 0.0, 4.0, 4).unwrap_err();
    assert!(why.contains("8 picks") && why.contains("28 mm"), "{why}");
    assert!(drag_of(&strip("A", 3.5, 0.0, 10), 3.5, 0.0, 4.0, 8).is_ok());
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, 101.0, 0.0001] {
        assert!(
            drag_of(&strip("A", 2.0, 0.0, 1), bad, 0.0, 4.0, 4).is_err(),
            "{bad}"
        );
    }
    assert!(drag_of(&strip("A", 2.0, 0.0, 1), 2.0, 0.0, 0.0, 4).is_err());
}

#[test]
fn pitches_from_a_fixed_seed_keep_the_stroke_whole_in_holes_and_in_pockets() {
    let mut seed: u64 = 4242;
    let mut next = |n: u64| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) % n
    };
    let mut fine = 0;
    for _ in 0..400 {
        let pitch = (next(80) + 1) as f64 * 0.5;
        let hole = [2.0, 4.0, 8.0][next(3) as usize];
        let count = next(500) as u32;
        let f = strip("S", pitch, next(360) as f64, count);
        let Ok(d) = drag_of(&f, pitch, 0.0, hole, 6) else {
            continue;
        };
        fine += 1;
        // the stroke is whole holes and whole pockets, and no shorter one is
        assert!(
            (d.stroke_mm - d.holes_per_stroke as f64 * hole).abs() < 1e-6,
            "{pitch} {hole}"
        );
        assert!(
            (d.stroke_mm - d.picks_per_stroke as f64 * pitch).abs() < 1e-6,
            "{pitch} {hole}"
        );
        for k in 1..d.picks_per_stroke {
            let shorter = k as f64 * pitch;
            let holes = shorter / hole;
            assert!(
                (holes - holes.round()).abs() > 1e-9,
                "{pitch} {hole}: {shorter} is whole holes"
            );
        }
        // the strokes empty the tape and not one more
        assert!(d.strokes * d.picks_per_stroke >= u64::from(count));
        assert!(d.strokes == 0 || (d.strokes - 1) * d.picks_per_stroke < u64::from(count));
        // the picks of one stroke stay inside the stroke
        assert_eq!(d.offsets.len() as u64, d.picks_per_stroke);
    }
    assert!(fine > 100, "{fine}");
}

#[test]
fn only_the_strips_of_a_table_are_looked_at_and_the_table_lists_them() {
    let specs = from_csv(&FEEDERS.replace("\r\n", "\n")).unwrap();
    let p = plan(&specs, 4.0, 4);
    assert_eq!((p.rows.len(), p.refused.len(), p.other), (3, 0, 1));
    assert_eq!(
        csv(&p),
        "feeder,pocket_pitch_mm,stroke_mm,holes_per_stroke,picks_per_stroke,strokes\nF1,2,4,1,2,50\nF2,4,4,1,1,40\nF3,3,12,3,4,15\n"
    );
    // the holes of another tape: every strip fits in a stroke of 12 mm or none does
    let tight = plan(&specs, 4.0, 1);
    assert_eq!(tight.rows.len(), 1);
    assert_eq!(tight.refused.len(), 2);
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

#[test]
fn the_command_ends_with_2_when_a_strip_cannot_be_dragged() {
    let o = run(&["drag-plan", "examples/feeders_drag.csv"]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stdout)
    );
    let text = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(text.contains("F3: pockets 3 mm apart, a stroke of 12 mm (3 holes) serves 4 picks, 15 strokes for the tape"), "{text}");
    assert!(
        text.contains("3 strips can be dragged, 0 cannot; 1 feeders are not strips"),
        "{text}"
    );
    let o = run(&["drag-plan", "examples/feeders_drag.csv", "--lang", "de"]);
    assert!(String::from_utf8_lossy(&o.stdout).contains("Streifen lassen sich ziehen"));
    // the most picks of a stroke is a setting of the machine
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_drag_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("machine.cfg");
    std::fs::write(&cfg, "drag_max_picks_per_stroke = 2\n").unwrap();
    let o = run(&[
        "drag-plan",
        "examples/feeders_drag.csv",
        "--config",
        cfg.to_str().unwrap(),
    ]);
    assert_eq!(
        o.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&o.stdout)
    );
    assert!(String::from_utf8_lossy(&o.stdout)
        .contains("F3: a pitch of 3 mm needs a stroke of 12 mm for 4 picks"));
    // the table, written once
    let table = dir.join("drag.csv");
    let args = [
        "drag-plan",
        "examples/feeders_drag.csv",
        "--out",
        table.to_str().unwrap(),
    ];
    assert_eq!(run(&args).status.code(), Some(0));
    assert!(std::fs::read_to_string(&table)
        .unwrap()
        .starts_with("feeder,pocket_pitch_mm"));
    assert_eq!(run(&args).status.code(), Some(1));
    assert_eq!(run(&["drag-plan"]).status.code(), Some(1));
    assert_eq!(
        run(&["drag-plan", "examples/board.pos"]).status.code(),
        Some(1)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
