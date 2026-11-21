//! The height of a bent board from a grid of measured points: bilinear interpolation that returns the measured value on every point and a plane exactly, the correction of a placement against
//! the zero, places outside the grid told and never guessed, a grid that is not whole refused, and the command ending with 2 when the board bends too much.

use eh3d_phoenix_pnp::heightmap::{corrections, csv, parse, MAX_POINTS};
use eh3d_phoenix_pnp::parts::{Part, Side};
use std::process::{Command, Output};

const GRID: &str = include_str!("../examples/height_grid.csv");

fn lf(text: &str) -> String {
    text.replace("\r\n", "\n")
}

fn part(reference: &str, x: f64, y: f64) -> Part {
    Part {
        reference: reference.to_string(),
        value: "10k".into(),
        package: "R_0603".into(),
        x,
        y,
        rotation: 0.0,
        side: Side::Top,
    }
}

#[test]
fn every_measured_point_gives_its_own_height_and_the_middle_gives_the_mean() {
    let g = parse(&lf(GRID)).unwrap();
    assert_eq!((g.xs.len(), g.ys.len(), g.points()), (3, 3, 9));
    for (iy, y) in g.ys.iter().enumerate() {
        for (ix, x) in g.xs.iter().enumerate() {
            let z = g.z_at(*x, *y).unwrap();
            assert!((z - g.z[iy][ix]).abs() < 1e-12, "{x},{y}");
        }
    }
    // half way along the first row between 0.00 and 0.04
    assert!((g.z_at(12.5, 0.0).unwrap() - 0.02).abs() < 1e-9);
    // the middle of a cell is the mean of its four corners
    let mean = (0.00 + 0.04 + 0.02 + 0.10) / 4.0;
    assert!((g.z_at(12.5, 10.0).unwrap() - mean).abs() < 1e-9);
    assert!((g.warp() - 0.30).abs() < 1e-9);
    assert_eq!(g.default_zero(), (0.0, 0.0));
}

#[test]
fn a_flat_tilted_board_is_followed_exactly_from_a_fixed_seed() {
    // z = a + b x + c y is a plane: bilinear interpolation gives it back at any place of the grid
    let (a, b, c) = (0.2, 0.003, -0.002);
    let mut text = String::from("x_mm,y_mm,z_mm\n");
    for y in [0.0, 30.0, 60.0, 100.0] {
        for x in [0.0, 40.0, 70.0, 120.0] {
            text.push_str(&format!("{x},{y},{}\n", a + b * x + c * y));
        }
    }
    let g = parse(&text).unwrap();
    let mut seed: u64 = 777;
    let mut next = |n: f64| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((seed >> 11) as f64 / (1u64 << 53) as f64) * n
    };
    for _ in 0..500 {
        let (x, y) = (next(120.0), next(100.0));
        let z = g.z_at(x, y).unwrap();
        assert!((z - (a + b * x + c * y)).abs() < 1e-9, "{x},{y}");
    }
}

#[test]
fn a_place_outside_the_grid_has_no_height_and_the_zero_must_be_inside() {
    let g = parse(&lf(GRID)).unwrap();
    for (x, y) in [
        (-0.01, 0.0),
        (50.5, 20.0),
        (10.0, 40.5),
        (f64::NAN, 1.0),
        (1.0, f64::INFINITY),
    ] {
        assert!(g.z_at(x, y).is_none(), "{x},{y}");
    }
    let parts = [part("A", 10.0, 10.0), part("B", 80.0, 10.0)];
    let list = corrections(&g, &parts, (0.0, 0.0)).unwrap();
    assert!(list[0].dz_mm.is_some());
    assert!(list[1].dz_mm.is_none());
    assert!(corrections(&g, &parts, (-5.0, 0.0)).is_err());
    // the table leaves the cell of a part outside empty and never writes -0
    let t = csv(&list);
    assert!(
        t.starts_with("reference,x_mm,y_mm,dz_mm\nA,10.000,10.000,"),
        "{t}"
    );
    assert!(t.contains("B,80.000,10.000,\n"), "{t}");
    let zero = csv(&corrections(&g, &[part("Z", 0.0, 0.0)], (0.0, 0.0)).unwrap());
    assert!(zero.contains("Z,0.000,0.000,0.000"), "{zero}");
}

#[test]
fn a_correction_is_the_height_at_the_place_less_the_height_at_the_zero() {
    let g = parse(&lf(GRID)).unwrap();
    let list = corrections(
        &g,
        &[part("A", 50.0, 40.0), part("B", 0.0, 0.0)],
        (25.0, 20.0),
    )
    .unwrap();
    assert!((list[0].dz_mm.unwrap() - (0.30 - 0.10)).abs() < 1e-9);
    assert!((list[1].dz_mm.unwrap() - (0.0 - 0.10)).abs() < 1e-9);
}

#[test]
fn a_grid_that_is_not_whole_or_not_a_table_of_heights_is_refused() {
    let head = "x_mm,y_mm,z_mm\n";
    for (text, why) in [
        ("", "empty"),
        ("x,y,z\n0,0,0\n", "header"),
        (&format!("{head}0,0,0\n1,0,0\n0,1,0\n"), "not a whole grid"),
        (&format!("{head}0,0,0\n1,0,0\n"), "two rows"),
        (&format!("{head}0,0,0\n0,0,1\n0,1,0\n1,1,0\n"), "twice"),
        (
            &format!("{head}0,0,x\n1,0,0\n0,1,0\n1,1,0\n"),
            "not a number",
        ),
        (
            &format!("{head}0,0,0\n1,0,0\n0,1,0\n1,1,9999\n"),
            "too high",
        ),
        (&format!("{head}0,0\n"), "fields"),
    ] {
        assert!(parse(text).is_err(), "{why}");
    }
    // a thousandth of a millimetre apart is another line, the same to it is the same point
    assert!(parse(&format!("{head}0,0,0\n1,0,0\n0,1,0\n1,1,0\n")).is_ok());
    assert!(parse(&format!("{head}0,0,0\n0.0001,0,1\n0,1,0\n1,1,0\n")).is_err());
    // the size is bounded
    let mut big = String::from(head);
    for i in 0..=MAX_POINTS {
        big.push_str(&format!("{i},0,0\n"));
    }
    assert!(parse(&big).is_err());
}

#[test]
fn a_damaged_table_from_a_fixed_seed_never_panics() {
    let mut seed: u64 = 99;
    let mut next = |n: u64| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) % n
    };
    let pieces = [
        "0,0,0",
        "1,0,0.1",
        "0,1,0.2",
        "1,1,0.3",
        "x,y,z",
        ",,",
        "1e999,0,0",
        "-1,-1,-1",
        "\"1,2\",3,4",
    ];
    let mut read = 0;
    for _ in 0..300 {
        // a grid of four points with a few lines changed, cut or added
        let mut lines: Vec<String> = ["0,0,0", "1,0,0.1", "0,1,0.2", "1,1,0.3"]
            .iter()
            .map(|l| l.to_string())
            .collect();
        for _ in 0..next(3) {
            let piece = pieces[next(pieces.len() as u64) as usize].to_string();
            match next(3) {
                0 => lines.push(piece),
                1 => {
                    let at = next(lines.len() as u64) as usize;
                    lines[at] = piece;
                }
                _ => {
                    lines.remove(next(lines.len() as u64) as usize);
                }
            }
        }
        let text = format!(
            "x_mm,y_mm,z_mm
{}
",
            lines.join(
                "
"
            )
        );
        if let Ok(g) = parse(&text) {
            read += 1;
            assert!(g.z_at(g.xs[0], g.ys[0]).is_some());
        }
    }
    assert!(read > 5, "{read}");
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn the_command_says_how_much_the_board_bends_and_how_much_each_height_changes() {
    let o = run(&[
        "height-map",
        "examples/height_grid.csv",
        "--parts",
        "examples/board.pos",
    ]);
    assert_eq!(o.status.code(), Some(0), "{}", out(&o));
    let text = out(&o);
    assert!(
        text.contains(
            "9 points (3 columns by 3 rows); the board bends 0.300 mm (the most allowed is 1 mm)"
        ),
        "{text}"
    );
    assert!(text.contains("changes from 0.052 to 0.181 mm"), "{text}");
    let o = run(&["height-map", "examples/height_grid.csv", "--lang", "ja"]);
    assert!(out(&o).contains("基板の反りは 0.3"), "{}", out(&o));
}

#[test]
fn the_command_ends_with_2_when_the_board_bends_too_much_or_a_part_is_outside() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_hm_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("machine.cfg");
    std::fs::write(&cfg, "heightmap_max_warp_mm = 0.2\n").unwrap();
    let c = cfg.to_str().unwrap();
    let o = run(&["height-map", "examples/height_grid.csv", "--config", c]);
    assert_eq!(o.status.code(), Some(2), "{}", out(&o));
    assert!(String::from_utf8_lossy(&o.stderr).contains("heightmap_max_warp_mm"));
    // a part far outside the grid
    let far = dir.join("far.pos");
    std::fs::write(
        &far,
        "ref,val,package,x,y,rot,side\nR1,10k,R_0603,200,200,0,top\n",
    )
    .unwrap();
    let o = run(&[
        "height-map",
        "examples/height_grid.csv",
        "--parts",
        far.to_str().unwrap(),
    ]);
    assert_eq!(o.status.code(), Some(2), "{}", out(&o));
    assert!(String::from_utf8_lossy(&o.stderr).contains("outside the measured grid"));
    // --out needs the parts, a file that is not a grid is an error of the command
    assert_eq!(
        run(&["height-map", "examples/height_grid.csv", "--out", "x.csv"])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        run(&["height-map", "examples/board.pos"]).status.code(),
        Some(1)
    );
    assert_eq!(
        run(&[
            "height-map",
            "examples/height_grid.csv",
            "--zero",
            "400,0",
            "--parts",
            "examples/board.pos"
        ])
        .status
        .code(),
        Some(1)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_new_settings_have_their_range() {
    use eh3d_phoenix_pnp::config::MachineConfig;
    for bad in [
        "heightmap_max_warp_mm = 0.01",
        "heightmap_max_warp_mm = 21",
        "heightmap_max_warp_mm = x",
        "drag_hole_pitch_mm = 0.5",
        "drag_hole_pitch_mm = 17",
        "drag_max_picks_per_stroke = 0",
        "drag_max_picks_per_stroke = 17",
        "drag_max_picks_per_stroke = 1.5",
    ] {
        assert!(MachineConfig::parse(bad).is_err(), "{bad}");
    }
    let c = MachineConfig::parse(
        "heightmap_max_warp_mm = 0.5\ndrag_hole_pitch_mm = 8\ndrag_max_picks_per_stroke = 6\n",
    )
    .unwrap();
    assert_eq!(
        (
            c.heightmap_max_warp_mm,
            c.drag_hole_pitch_mm,
            c.drag_max_picks_per_stroke
        ),
        (0.5, 8.0, 6)
    );
}
