//! What a table of parts holds and the parts that sit too near each other: the counts, the box, the pairs found with a grid (checked against the plain comparison of every pair), the limits and `board-stats`.

use eh3d_phoenix_pnp::boardstats::{near, near_text, stats, NEAR_RANGE_MM};
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::parts::{from_csv, Part, Side};
use eh3d_phoenix_pnp::sim::Rng;
use std::path::PathBuf;
use std::process::{Command, Output};

const BOARD: &str = "ref,val,package,x,y,rot,side\nR1,10k,R_0603,10,10,0,top\nR2,10k,R_0603,20,10,90,top\nR3,22k,r_0603,30,10,180,top\nC1,100n,C_0603,10,20,0,top\nU1,MCU,QFN-32,40,15,0,top\nD1,LED,LED_0603,5,30,0,bottom\n";

fn parts(text: &str) -> Vec<Part> {
    from_csv(text).unwrap()
}

#[test]
fn the_table_is_counted_by_side_kind_and_package() {
    let s = stats(&parts(BOARD), 2);
    assert_eq!((s.total, s.top, s.bottom, s.not_mounted), (6, 5, 1, 2));
    // the kinds are a value and a package, whatever the case of the package
    assert_eq!(s.values, 5);
    let packages: Vec<(&str, usize, usize)> = s
        .packages
        .iter()
        .map(|p| (p.package.as_str(), p.count, p.values))
        .collect();
    assert_eq!(
        packages,
        vec![
            ("R_0603", 3, 2),
            ("C_0603", 1, 1),
            ("LED_0603", 1, 1),
            ("QFN-32", 1, 1)
        ]
    );
    let text = s.text();
    assert!(text.starts_with(
        "6 parts (5 on the top, 1 on the bottom), 2 not to be mounted, 5 kinds of part"
    ));
    assert!(text.contains("package R_0603: 3 parts, 2 values\n"));
}

#[test]
fn the_box_of_the_centres_and_an_empty_table() {
    let s = stats(&parts(BOARD), 0);
    assert_eq!(s.corners, Some(((5.0, 10.0), (40.0, 30.0))));
    assert_eq!(s.size(), Some((35.0, 20.0)));
    assert!(s
        .text()
        .contains("centres from 5.000,10.000 to 40.000,30.000: 35.000 x 20.000 mm\n"));
    let empty = stats(&[], 0);
    assert_eq!((empty.total, empty.corners, empty.size()), (0, None, None));
    assert_eq!(
        empty.text(),
        "0 parts (0 on the top, 0 on the bottom), 0 not to be mounted, 0 kinds of part (value and package)\n"
    );
}

#[test]
fn parts_that_are_too_near_are_found_on_one_side_only_and_the_nearest_first() {
    let table = "ref,val,package,x,y,rot,side\nA1,1,R,10,10,0,top\nA2,1,R,10.3,10,0,top\nA3,1,R,10,10,0,top\nB1,1,R,10.1,10,0,bottom\nC1,1,R,50,50,0,top\nC2,1,R,50.45,50,0,top\n";
    let pairs = near(&parts(table), 0.5).unwrap();
    let shown: Vec<(String, String, String)> = pairs
        .iter()
        .map(|n| {
            (
                n.first.clone(),
                n.second.clone(),
                format!("{:.2}", n.dist_mm),
            )
        })
        .collect();
    let want = [
        ("A1", "A3", "0.00"),
        ("A1", "A2", "0.30"),
        ("A2", "A3", "0.30"),
        ("C1", "C2", "0.45"),
    ];
    assert_eq!(shown.len(), want.len(), "{shown:?}");
    for (got, w) in shown.iter().zip(want) {
        assert_eq!((got.0.as_str(), got.1.as_str(), got.2.as_str()), w);
    }
    // the part on the other side is near none of them
    assert!(pairs.iter().all(|n| n.first != "B1" && n.second != "B1"));
    // the distance is a limit: a pair at exactly that distance is not under it
    let two = "ref,val,package,x,y,rot,side\nA,1,R,0,0,0,top\nB,1,R,0.5,0,0,top\n";
    assert!(near(&parts(two), 0.5).unwrap().is_empty());
    assert_eq!(near(&parts(two), 0.6).unwrap().len(), 1);
}

#[test]
fn the_grid_finds_what_the_plain_comparison_of_every_pair_finds() {
    // a fixed seed, so that the numbers are the same on every run: parts on both sides, many near, some in the same place
    let mut rng = Rng::new(7);
    let mut p: Vec<Part> = Vec::new();
    for i in 0..400 {
        // coordinates in steps of 0.05 mm over a board of 12 mm, so that distances repeat and cells meet
        let x = (rng.next_u64() % 240) as f64 * 0.05;
        let y = (rng.next_u64() % 240) as f64 * 0.05;
        p.push(Part {
            reference: format!("P{i:03}"),
            value: "1".into(),
            package: "R".into(),
            x,
            y,
            rotation: 0.0,
            side: if i % 3 == 0 { Side::Bottom } else { Side::Top },
        });
    }
    for within in [0.2, 0.5, 1.0, 3.3] {
        let got = near(&p, within).unwrap();
        let mut want: Vec<(String, String)> = Vec::new();
        for i in 0..p.len() {
            for j in i + 1..p.len() {
                if p[i].side == p[j].side && (p[i].x - p[j].x).hypot(p[i].y - p[j].y) < within {
                    want.push((p[i].reference.clone(), p[j].reference.clone()));
                }
            }
        }
        let mut have: Vec<(String, String)> = got
            .iter()
            .map(|n| (n.first.clone(), n.second.clone()))
            .collect();
        have.sort();
        want.sort();
        assert_eq!(have, want, "within {within}");
        // the nearest first
        assert!(got.windows(2).all(|w| w[0].dist_mm <= w[1].dist_mm));
    }
}

#[test]
fn the_distance_has_a_range_and_a_table_without_places_is_not_a_problem() {
    for bad in [0.0, 0.009, 50.1, -1.0, f64::NAN, f64::INFINITY] {
        let e = near(&parts(BOARD), bad).unwrap_err();
        assert_eq!(e.code, Code::PlanSetting, "{bad}");
    }
    assert!(near(&parts(BOARD), NEAR_RANGE_MM.0).unwrap().is_empty());
    assert_eq!(near(&parts(BOARD), NEAR_RANGE_MM.1).unwrap().len(), 10);
    assert!(near(&[], 1.0).unwrap().is_empty());
}

#[test]
fn the_pairs_are_cut_at_a_limit_and_the_line_says_how_many_there_were() {
    let table = "ref,val,package,x,y,rot,side\nA,1,R,0,0,0,top\nB,1,R,0.1,0,0,top\nC,1,R,0.2,0,0,top\nD,1,R,0.3,0,0,top\n";
    let pairs = near(&parts(table), 1.0).unwrap();
    assert_eq!(pairs.len(), 6);
    let text = near_text(&pairs, 1.0, 2);
    assert_eq!(text.lines().count(), 4, "{text}");
    assert!(text.contains("... and 4 more\n"));
    assert!(text.ends_with("6 pairs of parts closer than 1 mm\n"));
    assert!(text.starts_with("C and D are 0.100 mm apart\n"));
    assert_eq!(
        near_text(&[], 0.5, 20),
        "0 pairs of parts closer than 0.5 mm\n"
    );
}

// ---- board-stats

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .current_dir(root())
        .output()
        .unwrap()
}

#[test]
fn the_command_prints_the_counts_and_exits_with_1_only_when_near_finds_a_pair() {
    let o = run(&["board-stats", "examples/board.pos"]);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(
        out.contains("7 parts (6 on the top, 1 on the bottom)"),
        "{out}"
    );
    assert!(out.contains("package R_0603: 3 parts, 1 values"), "{out}");
    assert!(!out.contains("pairs of parts"), "{out}");
    // the parts of the example are 5 mm apart
    let o = run(&["board-stats", "examples/board.pos", "--near", "4"]);
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("0 pairs of parts closer than 4 mm"));
    let o = run(&[
        "board-stats",
        "examples/board.pos",
        "--near",
        "5.5",
        "--limit",
        "1",
    ]);
    assert!(!o.status.success());
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(out.contains("C1 and C2 are 5.000 mm apart\n"), "{out}");
    assert!(out.contains("... and 2 more\n"), "{out}");
    assert!(String::from_utf8_lossy(&o.stderr).contains("3 pairs of parts are too near"));
    // bad words
    assert!(!run(&["board-stats"]).status.success());
    assert!(!run(&["board-stats", "examples/board.pos", "--near", "0"])
        .status
        .success());
    assert!(!run(&["board-stats", "examples/board.pos", "--near", "x"])
        .status
        .success());
    assert!(
        !run(&["board-stats", "examples/board.pos", "--limit", "-1"])
            .status
            .success()
    );
    assert!(!run(&["board-stats", "examples/board.pos", "--other"])
        .status
        .success());
}
