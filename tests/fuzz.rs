//! Chance on a fixed seed: every reader of a file is given the example files damaged in many ways and must answer with a value or an error, never a panic; and boards made by chance keep
//! the properties that the tools promise (what is written is read back, a transform keeps the distances, a split gives every part once, the odds stay between 0 and 1, a request that
//! is made is taken by the server in memory). The numbers are the same on every run, so a failure can be repeated: the seed is in its message.

use eh3d_phoenix_pnp::asmexport::{write, Format, Sides, Unit};
use eh3d_phoenix_pnp::calibrate::parse_camera_points;
use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::exchange;
use eh3d_phoenix_pnp::failodds::{analyse, parse_rules};
use eh3d_phoenix_pnp::feeding::{assign, from_csv as feeders_from_csv, FeederSpec};
use eh3d_phoenix_pnp::lottrace::{parse_lots, trace};
use eh3d_phoenix_pnp::panel::Panel;
use eh3d_phoenix_pnp::parts::{parse, Part, Side};
use eh3d_phoenix_pnp::recipe::{Limits, Recipe};
use eh3d_phoenix_pnp::refill::parse_refills;
use eh3d_phoenix_pnp::reorient::{apply, Transform};
use eh3d_phoenix_pnp::server::{
    feeder_rows, feeders_request, parse_response, placements, Order, Remote, SimServer, Target,
};
use eh3d_phoenix_pnp::sim::Rng;
use eh3d_phoenix_pnp::splitjob::split;
use std::panic::{catch_unwind, AssertUnwindSafe};

/// A number from 0 up to, not including, `n`.
fn below(rng: &mut Rng, n: usize) -> usize {
    (rng.next_u64() % n.max(1) as u64) as usize
}

const NASTY: [&str; 18] = [
    "\"",
    ",",
    ";",
    "\n",
    "\r\n",
    "\t",
    "\u{0}",
    "\u{7}",
    "é",
    "日本",
    "\u{feff}",
    "=",
    "#",
    " ",
    "1e999",
    "-0",
    "NaN",
    "99999999999999999999",
];

/// The text with a few damages made by chance: a piece cut out, a nasty piece put in, a line twice or swapped, a number made huge, the end cut off.
fn damaged(rng: &mut Rng, base: &str) -> String {
    let mut chars: Vec<char> = base.chars().collect();
    for _ in 0..1 + below(rng, 6) {
        if chars.is_empty() {
            break;
        }
        let at = below(rng, chars.len());
        match below(rng, 6) {
            0 => {
                let end = (at + 1 + below(rng, 12)).min(chars.len());
                chars.drain(at..end);
            }
            1 => {
                let piece: Vec<char> = NASTY[below(rng, NASTY.len())].chars().collect();
                for (k, c) in piece.into_iter().enumerate() {
                    chars.insert(at + k, c);
                }
            }
            2 => {
                let text: String = chars.iter().collect();
                let lines: Vec<&str> = text.lines().collect();
                if !lines.is_empty() {
                    let l = below(rng, lines.len());
                    let mut out: Vec<&str> = lines.clone();
                    out.insert(l, lines[l]);
                    chars = out.join("\n").chars().collect();
                }
            }
            3 => {
                let text: String = chars.iter().collect();
                let mut lines: Vec<&str> = text.lines().collect();
                if lines.len() > 1 {
                    let (a, b) = (below(rng, lines.len()), below(rng, lines.len()));
                    lines.swap(a, b);
                    chars = lines.join("\n").chars().collect();
                }
            }
            4 => {
                if let Some(d) = chars.iter().position(char::is_ascii_digit) {
                    let from = d.max(at.min(chars.len() - 1));
                    if let Some(p) = chars[from..].iter().position(char::is_ascii_digit) {
                        for (k, c) in "9999999999999999".chars().enumerate() {
                            chars.insert(from + p + k, c);
                        }
                    }
                }
            }
            _ => chars.truncate(at),
        }
    }
    chars.into_iter().collect()
}

/// Every reader, on a text: it must not panic.
fn read_everything(text: &str) {
    let _ = parse(text);
    let _ = feeders_from_csv(text);
    let _ = MachineConfig::parse(text);
    let _ = Panel::parse(text);
    let _ = exchange::parse(text);
    let _ = Recipe::parse(text, &Limits::default());
    let _ = parse_camera_points(text);
    let _ = parse_lots(text);
    let _ = parse_rules(text);
    let _ = parse_refills(text);
    let mut catalog = Catalog::new();
    let _ = catalog.add_packages(text);
    let _ = catalog.add_parts(text);
}

const BASES: [(&str, &str); 12] = [
    ("board.pos", include_str!("../examples/board.pos")),
    ("board_dnp.csv", include_str!("../examples/board_dnp.csv")),
    ("centroid.csv", include_str!("../examples/centroid.csv")),
    ("feeders.csv", include_str!("../examples/feeders.csv")),
    (
        "feeders_kinds.csv",
        include_str!("../examples/feeders_kinds.csv"),
    ),
    ("machine.cfg", include_str!("../examples/machine.cfg")),
    ("panel.json", include_str!("../examples/panel.json")),
    ("job.recipe", include_str!("../examples/job.recipe")),
    (
        "camera_points.csv",
        include_str!("../examples/camera_points.csv"),
    ),
    ("lots.csv", include_str!("../examples/lots.csv")),
    (
        "failure_rules.csv",
        include_str!("../examples/failure_rules.csv"),
    ),
    ("packages.csv", include_str!("../examples/packages.csv")),
];

#[test]
fn no_reader_panics_on_the_example_files_damaged_by_chance() {
    for (name, base) in BASES {
        for seed in 0..120u64 {
            let mut rng = Rng::new(seed * 1000 + name.len() as u64);
            let text = damaged(&mut rng, base);
            let result = catch_unwind(AssertUnwindSafe(|| read_everything(&text)));
            assert!(
                result.is_ok(),
                "{name} seed {seed} makes a reader panic:\n{text:?}"
            );
        }
    }
}

#[test]
fn no_reader_panics_on_bytes_made_by_chance() {
    for seed in 0..300u64 {
        let mut rng = Rng::new(seed);
        let len = below(&mut rng, 300);
        let bytes: Vec<u8> = (0..len).map(|_| rng.next_u64() as u8).collect();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let result = catch_unwind(AssertUnwindSafe(|| {
            read_everything(&text);
            let _ = parse_response(&bytes);
        }));
        assert!(result.is_ok(), "seed {seed} makes a reader panic");
    }
}

#[test]
fn the_reader_of_server_answers_never_panics_on_answers_cut_or_mixed_up() {
    let good = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n6\r\n{\"a\":1\r\n1\r\n}\r\n0\r\n\r\n";
    for seed in 0..400u64 {
        let mut rng = Rng::new(seed);
        let mut bytes = good.to_vec();
        for _ in 0..1 + below(&mut rng, 4) {
            match below(&mut rng, 3) {
                0 => {
                    let at = below(&mut rng, bytes.len());
                    bytes.truncate(at);
                }
                1 if !bytes.is_empty() => {
                    let at = below(&mut rng, bytes.len());
                    bytes[at] = rng.next_u64() as u8;
                }
                _ if !bytes.is_empty() => {
                    let at = below(&mut rng, bytes.len());
                    bytes.insert(at, b'\r');
                }
                _ => {}
            }
        }
        assert!(
            catch_unwind(|| parse_response(&bytes)).is_ok(),
            "seed {seed}: {bytes:?}"
        );
    }
}

/// A board made by chance: references that are all different, values and packages from a few, places in a box.
fn random_board(rng: &mut Rng, base: usize, span: usize) -> (Vec<Part>, Vec<FeederSpec>) {
    let n = base + below(rng, span);
    let kinds = [
        ("10k", "R_0603"),
        ("100n", "C_0603"),
        ("MCU", "QFN-32"),
        ("LED", "LED_0603"),
        ("1u", "C_0805"),
    ];
    let parts: Vec<Part> = (0..n)
        .map(|i| {
            let (value, package) = kinds[below(rng, kinds.len())];
            Part {
                reference: format!("X{i}"),
                value: value.to_string(),
                package: package.to_string(),
                x: (below(rng, 200_000) as f64 - 100_000.0) / 1000.0,
                y: (below(rng, 200_000) as f64 - 100_000.0) / 1000.0,
                rotation: below(rng, 360_000) as f64 / 1000.0,
                side: if below(rng, 4) == 0 {
                    Side::Bottom
                } else {
                    Side::Top
                },
            }
        })
        .collect();
    let mut csv = String::from("id,value,package,x,y,count\n");
    for (i, (value, package)) in kinds.iter().enumerate() {
        csv.push_str(&format!("F{i},{value},{package},0,{i},100000\n"));
    }
    (parts, feeders_from_csv(&csv).unwrap())
}

fn same_parts(a: &[Part], b: &[Part], tol: f64) -> bool {
    let (mut a, mut b) = (a.to_vec(), b.to_vec());
    a.sort_by(|x, y| x.reference.cmp(&y.reference));
    b.sort_by(|x, y| x.reference.cmp(&y.reference));
    a.len() == b.len()
        && a.iter().zip(&b).all(|(p, q)| {
            let d = (p.rotation - q.rotation).rem_euclid(360.0);
            p.reference == q.reference
                && p.value == q.value
                && p.package == q.package
                && p.side == q.side
                && (p.x - q.x).abs() <= tol
                && (p.y - q.y).abs() <= tol
                && (d <= tol || 360.0 - d <= tol)
        })
}

#[test]
fn a_board_that_is_written_is_read_back_whatever_the_shape_and_the_unit() {
    for seed in 0..60u64 {
        let mut rng = Rng::new(seed);
        let (parts, _) = random_board(&mut rng, 1, 60);
        for format in [Format::KicadPos, Format::Generic] {
            for unit in [Unit::Mm, Unit::Inch] {
                let text = write(&parts, format, unit, Sides::All);
                let back =
                    parse(&text).unwrap_or_else(|e| panic!("seed {seed} {format:?} {unit:?}: {e}"));
                assert!(
                    same_parts(&parts, &back, 1e-4),
                    "seed {seed} {format:?} {unit:?}"
                );
            }
        }
    }
}

#[test]
fn a_transform_keeps_the_distances_and_its_opposite_gives_the_board_back() {
    for seed in 0..80u64 {
        let mut rng = Rng::new(seed);
        let (parts, _) = random_board(&mut rng, 2, 40);
        let deg = below(&mut rng, 720_000) as f64 / 1000.0 - 360.0;
        let shift = (
            below(&mut rng, 2000) as f64 - 1000.0,
            below(&mut rng, 2000) as f64 - 1000.0,
        );
        let flip = below(&mut rng, 2) == 0;
        let t = Transform {
            rotate_deg: deg,
            flip,
            shift,
            zero: false,
        };
        let moved = apply(&parts, &t).unwrap();
        for i in 1..parts.len() {
            let d = |p: &Part, q: &Part| (p.x - q.x).hypot(p.y - q.y);
            assert!(
                (d(&parts[i], &parts[0]) - d(&moved[i], &moved[0])).abs() < 1e-6,
                "seed {seed}"
            );
        }
        // back: take the shift off, turn the other way, flip again
        let unshifted = apply(
            &moved,
            &Transform {
                shift: (-shift.0, -shift.1),
                ..Transform::default()
            },
        )
        .unwrap();
        let unturned = apply(
            &unshifted,
            &Transform {
                rotate_deg: -deg,
                ..Transform::default()
            },
        )
        .unwrap();
        let back = if flip {
            apply(
                &unturned,
                &Transform {
                    flip: true,
                    ..Transform::default()
                },
            )
            .unwrap()
        } else {
            unturned
        };
        assert!(same_parts(&parts, &back, 1e-6), "seed {seed}");
    }
}

#[test]
fn a_split_gives_every_part_to_one_machine_and_the_work_is_never_further_apart_than_the_biggest_feeder(
) {
    for seed in 0..60u64 {
        let mut rng = Rng::new(seed);
        let (parts, feeders) = random_board(&mut rng, 5, 80);
        let picks = assign(&parts, &feeders, None);
        let machines = 1 + below(&mut rng, 6);
        let s = split(&parts, &feeders, &picks, machines, 1.0);
        assert_eq!(s.machines.len(), machines);
        let mut refs: Vec<&str> = s.machines.iter().flat_map(|m| m.references()).collect();
        refs.sort_unstable();
        let before = refs.len();
        refs.dedup();
        assert_eq!(
            (before, refs.len()),
            (parts.len(), parts.len()),
            "seed {seed}"
        );
        let biggest = feeders
            .iter()
            .map(|f| {
                picks
                    .picks
                    .iter()
                    .filter(|p| feeders[p.feeder].base.id == f.base.id)
                    .count()
            })
            .max()
            .unwrap() as f64;
        let (most, least) = s.extremes();
        assert!(
            most - least <= biggest,
            "seed {seed}: {most} {least} {biggest}"
        );
        assert_eq!(s, split(&parts, &feeders, &picks, machines, 1.0));
    }
}

#[test]
fn the_odds_stay_between_0_and_1_and_do_not_fall_when_a_retry_is_allowed() {
    for seed in 0..60u64 {
        let mut rng = Rng::new(seed);
        let (parts, feeders) = random_board(&mut rng, 1, 50);
        let picks = assign(&parts, &feeders, None);
        let rate = |rng: &mut Rng| below(rng, 1001) as f64 / 1000.0;
        let (feed, pick, place) = (rate(&mut rng), rate(&mut rng), rate(&mut rng));
        let mut last = -1.0;
        for retries in 0..5 {
            let cfg = MachineConfig::parse(&format!(
                "max_placement_attempts = 10\nsim_feed_fail_rate = {feed}\nsim_pick_fail_rate = {pick}\nsim_place_fail_rate = {place}\nfeed_retries = {retries}\npick_retries = {retries}\nplace_retries = {retries}\n"
            ))
            .unwrap();
            let o = analyse(&parts, &feeders, &picks, &cfg, &[]);
            assert!(
                (0.0..=1.0).contains(&o.board_ok) && o.board_ok.is_finite(),
                "seed {seed}"
            );
            assert!(
                o.failed >= 0.0
                    && o.failed <= parts.len() as f64 + 1e-9
                    && o.retries >= 0.0
                    && o.retry_s >= 0.0
            );
            assert!(
                o.board_ok + 1e-12 >= last,
                "seed {seed} retries {retries}: {} {last}",
                o.board_ok
            );
            last = o.board_ok;
        }
    }
}

#[test]
fn a_trace_names_a_lot_for_every_part_when_every_feeder_has_one() {
    for seed in 0..40u64 {
        let mut rng = Rng::new(seed);
        let (parts, feeders) = random_board(&mut rng, 1, 60);
        let picks = assign(&parts, &feeders, None);
        let lots: String = std::iter::once("feeder,lot".to_string())
            .chain((0..5).map(|i| format!("F{i},L{i}")))
            .collect::<Vec<_>>()
            .join("\n");
        let t = trace(&parts, &feeders, &picks, &parse_lots(&lots).unwrap());
        assert!(t.is_complete(), "seed {seed}");
        assert_eq!(t.rows.len(), parts.len());
        assert!(t
            .rows
            .iter()
            .all(|r| r.lot.as_ref().is_some_and(|l| l.feeder == r.feeder)));
        assert_eq!(
            t.lots.iter().map(|u| u.references.len()).sum::<usize>(),
            parts.len()
        );
    }
}

#[test]
fn a_request_that_is_made_is_taken_by_the_server_in_memory_and_every_part_is_placed() {
    let target = Target::new("sim", "", "m1", 5.0, 50, 1).unwrap();
    for seed in 0..40u64 {
        let mut rng = Rng::new(seed);
        let (parts, feeders) = random_board(&mut rng, 1, 70);
        let rows = feeder_rows(&feeders).unwrap();
        let made = placements(&parts, &rows, (0.0, 0.0)).unwrap();
        assert_eq!(
            (made.count, made.unmatched.len()),
            (parts.len(), 0),
            "seed {seed}"
        );
        let mut r = Remote::new(SimServer::new(&["m1"]), &target);
        r.transport.steps_per_read = 1 + below(&mut rng, 20) as u64;
        let load = r.create("feeders", &feeders_request(&rows, true)).unwrap();
        r.order(&load.id, Order::Start).unwrap();
        let job = r.create("board", &made.request).unwrap();
        assert_eq!(job.total as usize, parts.len());
        r.order(&job.id, Order::Start).unwrap();
        let end = r.follow(&job.id, 10_000, &mut |_| {}, &mut || {}).unwrap();
        assert_eq!(
            (end.state.as_str(), end.done as usize),
            ("done", parts.len()),
            "seed {seed}"
        );
        // what the machine took from its feeders is what the board needed
        let taken: u64 = (1..=rows.len() as u32)
            .map(|s| 100_000 - r.transport.feeder("m1", s).unwrap().1)
            .sum();
        assert_eq!(taken as usize, parts.len(), "seed {seed}");
    }
}
