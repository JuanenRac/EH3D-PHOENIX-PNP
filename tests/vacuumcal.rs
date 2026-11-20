//! The thresholds of the vacuum from readings, and the boards made up from a seed.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::feeders::{assign, from_csv as feeders_csv};
use eh3d_phoenix_pnp::parts::{parse, Side};
use eh3d_phoenix_pnp::plan::{plan, Settings};
use eh3d_phoenix_pnp::testboard::{make, Kind, Spec, MAX_PARTS};
use eh3d_phoenix_pnp::vacuumcal::{calibrate, parse as parse_readings, report, Note, Params};

const READINGS: &str = include_str!("../examples/vacuum_readings.csv");

fn params() -> Params {
    Params::from_config(&MachineConfig::default())
}

#[test]
fn the_thresholds_sit_a_margin_inside_the_gap() {
    let r = parse_readings(READINGS).unwrap();
    assert_eq!((r.free.len(), r.held.len(), r.released.len()), (5, 5, 5));
    let p = calibrate(&r, &params()).unwrap();
    let (pick, release) = p.thresholds.unwrap();
    // the strongest reading with no part is 10.5, the weakest with one 59.4: the gap is 48.9 and the margin 25 per cent of it
    assert!((pick - (59.4 - 0.25 * 48.9)).abs() < 1e-9, "{pick}");
    assert!((release - (10.5 + 0.25 * 48.9)).abs() < 1e-9, "{release}");
    assert!(release < pick);
    assert!(p
        .notes
        .iter()
        .all(|n| !matches!(n, Note::Overlap(..) | Note::SmallGap(_))));
}

#[test]
fn a_bigger_margin_keeps_the_thresholds_nearer_the_readings_and_never_crosses() {
    let r = parse_readings(READINGS).unwrap();
    for margin in [0.0, 10.0, 25.0, 45.0] {
        let p = calibrate(
            &r,
            &Params {
                margin_pct: margin,
                ..params()
            },
        )
        .unwrap();
        let (pick, release) = p.thresholds.unwrap();
        assert!(release < pick, "{margin}");
        assert!(pick <= 59.4 + 1e-9 && release >= 10.5 - 1e-9, "{margin}");
    }
}

#[test]
fn readings_that_overlap_give_no_threshold_and_say_so() {
    let r = parse_readings("state,kpa\nfree,8\nfree,35\nheld,30\nheld,60\nreleased,9\n").unwrap();
    let p = calibrate(&r, &params()).unwrap();
    assert!(p.thresholds.is_none());
    assert!(p.notes.iter().any(|n| matches!(n, Note::Overlap(..))));
    let t = report(&p, Lang::En);
    assert!(t.contains("overlap") && !t.contains("vacuum_pick_min_kpa"));
}

#[test]
fn a_small_gap_few_readings_and_the_current_figures_that_misread_are_told() {
    let r = parse_readings("state,kpa\nfree,8\nheld,15\nheld,16\n").unwrap();
    let p = calibrate(&r, &params()).unwrap();
    assert!(p.notes.iter().any(|n| matches!(n, Note::SmallGap(_))));
    assert!(p
        .notes
        .iter()
        .any(|n| matches!(n, Note::FewReadings("held", 2))));
    // the figures now in force (pick at 40) would call both held readings a miss
    assert!(
        p.notes
            .iter()
            .any(|n| matches!(n, Note::CurrentMisreads(2, 0))),
        "{:?}",
        p.notes
    );
}

#[test]
fn what_cannot_be_worked_out_is_an_error() {
    assert!(calibrate(&parse_readings("state,kpa\nfree,8\n").unwrap(), &params()).is_err());
    assert!(calibrate(&parse_readings("state,kpa\nheld,60\n").unwrap(), &params()).is_err());
    assert!(parse_readings("").is_err());
    assert!(parse_readings("state\nfree\n").is_err());
    assert!(parse_readings("state,kpa\nsideways,5\n")
        .unwrap_err()
        .contains("line 2"));
    assert!(parse_readings("state,kpa\nfree,abc\n")
        .unwrap_err()
        .contains("line 2"));
    assert!(parse_readings("state,kpa\nfree,1e999\n").is_err());
    assert!(parse_readings("state,kpa\nfree,NaN\n").is_err());
    // the synonyms and a semicolon table
    let r = parse_readings("State;kPa\nopen;8\nHOLDING;60\ndropped;9\n").unwrap();
    assert_eq!((r.free.len(), r.held.len(), r.released.len()), (1, 1, 1));
}

#[test]
fn the_report_is_in_the_eight_languages_and_has_the_two_lines_for_the_settings() {
    let p = calibrate(&parse_readings(READINGS).unwrap(), &params()).unwrap();
    let mut seen = std::collections::BTreeSet::new();
    for l in Lang::ALL {
        let t = report(&p, l);
        assert!(
            t.contains("vacuum_pick_min_kpa = 47.18")
                && t.contains("vacuum_release_max_kpa = 22.73"),
            "{t}"
        );
        seen.insert(t.lines().next().unwrap().to_string());
    }
    assert!(seen.len() >= 7);
    // the lines it writes are lines that the settings take
    let t = report(&p, Lang::En);
    let lines: String = t
        .lines()
        .filter(|l| l.starts_with("vacuum_"))
        .map(|l| format!("{l}\n"))
        .collect();
    let cfg = MachineConfig::parse(&lines).unwrap();
    assert!((cfg.vacuum_pick_min_kpa - 47.18).abs() < 1e-9);
}

#[test]
fn the_keys_of_the_vacuum_and_of_the_boards_have_their_ranges() {
    for (line, bad) in [
        ("vacuum_margin_pct = 46\n", true),
        ("vacuum_margin_pct = -1\n", true),
        ("vacuum_min_readings = 0\n", true),
        ("testboard_count = 0\n", true),
        ("testboard_count = 5001\n", true),
        ("testboard_pitch_mm = 0.1\n", true),
        ("vacuum_margin_pct = 45\n", false),
        ("testboard_count = 5000\n", false),
    ] {
        assert_eq!(MachineConfig::parse(line).is_err(), bad, "{line}");
    }
}

// ---- boards made up from a seed

fn spec(kind: Kind, seed: u64, count: usize) -> Spec {
    Spec {
        kind,
        seed,
        count,
        pitch_mm: 2.0,
    }
}

#[test]
fn the_same_seed_gives_the_same_bytes_and_another_seed_another_board() {
    for kind in [Kind::Grid, Kind::Mixed, Kind::Dense] {
        let a = make(&spec(kind, 7, 60)).unwrap();
        assert_eq!(a, make(&spec(kind, 7, 60)).unwrap());
    }
    let (a, b) = (
        make(&spec(Kind::Mixed, 1, 60)).unwrap(),
        make(&spec(Kind::Mixed, 2, 60)).unwrap(),
    );
    assert_ne!(a.parts_csv, b.parts_csv);
    // the grid has nothing to chance: one board
    assert_eq!(
        make(&spec(Kind::Grid, 1, 30)).unwrap().parts_csv,
        make(&spec(Kind::Grid, 99, 30)).unwrap().parts_csv
    );
}

#[test]
fn every_board_is_read_by_the_program_and_its_feeders_cover_it_and_it_can_be_planned() {
    for (kind, count) in [
        (Kind::Grid, 40),
        (Kind::Mixed, 200),
        (Kind::Dense, 100),
        (Kind::Mixed, 1),
    ] {
        let b = make(&spec(kind, 5, count)).unwrap();
        let parts = parse(&b.parts_csv).unwrap();
        assert_eq!(parts.len(), count);
        assert_eq!(b.parts, count);
        let feeders = feeders_csv(&b.feeders_csv).unwrap();
        assert_eq!(feeders.len(), b.feeders);
        let a = assign(&parts, &feeders);
        assert!(
            a.without_feeder.is_empty() && a.short.is_empty(),
            "{kind:?}"
        );
        let top = parts.iter().filter(|p| p.side == Side::Top).count();
        if top > 0 {
            let s = Settings::default();
            let a = eh3d_phoenix_pnp::feeders::assign_side(&parts, &feeders, Some(Side::Top));
            assert!(plan(&parts, &feeders, &a, &s).is_ok());
        }
    }
}

#[test]
fn a_mixed_board_turns_parts_in_the_four_directions_on_both_sides_and_keeps_them_apart() {
    let b = make(&spec(Kind::Mixed, 3, 300)).unwrap();
    let parts = parse(&b.parts_csv).unwrap();
    let rots: std::collections::BTreeSet<i64> = parts.iter().map(|p| p.rotation as i64).collect();
    assert_eq!(rots, [0, 90, 180, 270].into_iter().collect());
    assert!(
        parts.iter().any(|p| p.side == Side::Bottom) && parts.iter().any(|p| p.side == Side::Top)
    );
    let mut places = std::collections::BTreeSet::new();
    for p in &parts {
        assert!(
            places.insert(((p.x * 100.0) as i64, (p.y * 100.0) as i64)),
            "two parts at {} {}",
            p.x,
            p.y
        );
        assert!(p.x >= 0.0 && p.x <= b.size.0 && p.y >= 0.0 && p.y <= b.size.1);
    }
    assert!(Catalog::new().is_empty());
}

#[test]
fn a_dense_board_has_the_parts_closer_than_a_grid_with_the_same_pitch() {
    let g = make(&spec(Kind::Grid, 1, 100)).unwrap();
    let d = make(&spec(Kind::Dense, 1, 100)).unwrap();
    assert!(d.size.0 < g.size.0);
    // never closer than 0.8 mm
    let dd = make(&Spec {
        pitch_mm: 0.2,
        ..spec(Kind::Dense, 1, 100)
    })
    .unwrap();
    let parts = parse(&dd.parts_csv).unwrap();
    assert!((parts[1].x - parts[0].x).abs() >= 0.8 - 1e-9);
}

#[test]
fn a_count_or_a_pitch_out_of_range_is_refused() {
    assert!(make(&spec(Kind::Grid, 1, 0)).is_err());
    assert!(make(&spec(Kind::Grid, 1, MAX_PARTS + 1)).is_err());
    assert!(make(&spec(Kind::Grid, 1, MAX_PARTS)).is_ok());
    assert!(make(&Spec {
        pitch_mm: 0.1,
        ..spec(Kind::Grid, 1, 5)
    })
    .is_err());
    assert!(make(&Spec {
        pitch_mm: f64::NAN,
        ..spec(Kind::Grid, 1, 5)
    })
    .is_err());
    assert_eq!(Kind::parse("MIXED"), Some(Kind::Mixed));
    assert_eq!(Kind::parse("other"), None);
}
