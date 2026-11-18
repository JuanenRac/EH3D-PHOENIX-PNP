//! Whether a library of feeders agrees with itself: slots, pitches, cells, empty feeders, packages, places out of reach and places that clash.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::feeders::Feeder;
use eh3d_phoenix_pnp::feeding::{from_csv, FeederKind, FeederSpec};
use eh3d_phoenix_pnp::feedlib::{check, counts, text, Rules, Severity};

const EXAMPLE: &str = include_str!("../examples/feeders_kinds.csv");
const PACKAGES: &str = include_str!("../examples/packages.csv");
const MACHINE: &str = include_str!("../examples/machine.cfg");

fn tape(id: &str, x: f64, y: f64, count: u32) -> FeederSpec {
    FeederSpec::from_basic(Feeder {
        id: id.into(),
        value: "10k".into(),
        package: "R_0603".into(),
        x,
        y,
        count,
    })
}

fn messages(found: &[eh3d_phoenix_pnp::feedlib::Finding]) -> Vec<String> {
    found
        .iter()
        .map(|f| format!("{} {}", f.feeder, f.message))
        .collect()
}

#[test]
fn the_example_library_has_nothing_to_report() {
    let feeders = from_csv(EXAMPLE).unwrap();
    let mut catalog = Catalog::new();
    catalog.add_packages(PACKAGES).unwrap();
    let found = check(&feeders, Some(&catalog), &Rules::default());
    assert!(found.is_empty(), "{found:?}");
    assert_eq!(text(&found), "0 errors, 0 warnings\n");
}

#[test]
fn a_slot_given_twice_is_an_error_on_the_second_feeder() {
    let mut a = tape("F1", 0.0, 0.0, 10);
    let mut b = tape("F2", 50.0, 0.0, 10);
    a.slot = Some("S1".into());
    b.slot = Some("S1".into());
    let found = check(&[a, b], None, &Rules::default());
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        (found[0].severity, found[0].feeder.as_str()),
        (Severity::Error, "F2")
    );
    assert!(found[0].message.contains("'S1'") && found[0].message.contains("'F1'"));
}

#[test]
fn the_pitch_of_a_strip_is_a_whole_number_of_the_step() {
    let mut f = tape("F1", 0.0, 0.0, 10);
    for (pitch, warns) in [
        (4.0, false),
        (2.0, false),
        (12.0, false),
        (3.0, true),
        (4.04, false),
        (4.2, true),
        (1.0, true),
    ] {
        f.kind = FeederKind::Strip {
            pitch,
            rotation: 0.0,
        };
        let found = check(std::slice::from_ref(&f), None, &Rules::default());
        assert_eq!(!found.is_empty(), warns, "{pitch}: {found:?}");
        assert!(found.iter().all(|x| x.severity == Severity::Warning));
    }
    // a step of 0 does not check the pitch, and a step of 0.5 accepts 3.5
    f.kind = FeederKind::Strip {
        pitch: 3.0,
        rotation: 0.0,
    };
    let off = Rules {
        pitch_step_mm: 0.0,
        ..Rules::default()
    };
    assert!(check(std::slice::from_ref(&f), None, &off).is_empty());
    f.kind = FeederKind::Strip {
        pitch: 3.5,
        rotation: 0.0,
    };
    let half = Rules {
        pitch_step_mm: 0.5,
        ..Rules::default()
    };
    assert!(check(std::slice::from_ref(&f), None, &half).is_empty());
}

#[test]
fn a_tray_with_more_parts_than_cells_or_cells_too_small_is_reported() {
    let mut t = tape("T1", 0.0, 0.0, 20);
    t.kind = FeederKind::Tray {
        cols: 3,
        rows: 2,
        pitch_x: 12.0,
        pitch_y: 12.0,
        rotation: 0.0,
    };
    let found = check(std::slice::from_ref(&t), None, &Rules::default());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].severity, Severity::Error);
    assert!(found[0].message.contains("6 cells") && found[0].message.contains("20 parts"));
    t.base.count = 6;
    assert!(check(std::slice::from_ref(&t), None, &Rules::default()).is_empty());
    t.kind = FeederKind::Tray {
        cols: 3,
        rows: 2,
        pitch_x: 0.2,
        pitch_y: 12.0,
        rotation: 0.0,
    };
    let found = check(std::slice::from_ref(&t), None, &Rules::default());
    assert_eq!(
        (found.len(), found[0].severity),
        (1, Severity::Warning),
        "{found:?}"
    );
}

#[test]
fn an_empty_feeder_that_is_on_and_a_warning_that_is_always_on_are_reported() {
    let mut f = tape("F1", 0.0, 0.0, 0);
    let found = check(std::slice::from_ref(&f), None, &Rules::default());
    assert_eq!(found.len(), 1);
    assert!(found[0].message.contains("holds no parts"));
    f.enabled = false;
    assert!(check(std::slice::from_ref(&f), None, &Rules::default()).is_empty());
    f.enabled = true;
    f.base.count = 10;
    f.warn_below = 11;
    let found = check(std::slice::from_ref(&f), None, &Rules::default());
    assert_eq!(found.len(), 1);
    assert!(
        found[0].message.contains("warns from the start"),
        "{found:?}"
    );
    f.warn_below = 10;
    assert!(check(std::slice::from_ref(&f), None, &Rules::default()).is_empty());
}

#[test]
fn a_package_that_the_catalog_does_not_have_is_a_warning_only_when_there_is_a_catalog() {
    let f = tape("F1", 0.0, 0.0, 10);
    let mut catalog = Catalog::new();
    // an empty catalog knows nothing and says nothing
    assert!(check(std::slice::from_ref(&f), Some(&catalog), &Rules::default()).is_empty());
    catalog
        .add_packages(
            "package,height_mm,pick_retries,bin,nozzle_tip,rank,tape_mm\nC_0603,0.5,2,0,N45,0,8\n",
        )
        .unwrap();
    let found = check(std::slice::from_ref(&f), Some(&catalog), &Rules::default());
    assert_eq!(found.len(), 1);
    assert!(found[0].message.contains("'R_0603' is not in the catalog"));
    assert!(check(std::slice::from_ref(&f), None, &Rules::default()).is_empty());
}

#[test]
fn a_pick_place_outside_the_area_the_machine_reaches_is_an_error_also_at_the_end_of_a_strip() {
    let rules = Rules {
        x_max: 100.0,
        ..Rules::default()
    };
    let mut f = tape("F1", 120.0, 0.0, 10);
    let found = check(std::slice::from_ref(&f), None, &rules);
    assert_eq!((found.len(), found[0].severity), (1, Severity::Error));
    // a strip that starts inside and ends outside: 10 parts 4 mm apart from x = 70 end at x = 106
    f.base.x = 70.0;
    f.kind = FeederKind::Strip {
        pitch: 4.0,
        rotation: 0.0,
    };
    let found = check(std::slice::from_ref(&f), None, &rules);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].message.contains("outside the area"));
    f.base.count = 5;
    assert!(check(std::slice::from_ref(&f), None, &rules).is_empty());
}

#[test]
fn two_pick_places_on_top_of_each_other_are_an_error_on_the_later_feeder() {
    let feeders = [
        tape("F1", 10.0, 10.0, 10),
        tape("F2", 11.0, 10.5, 10),
        tape("F3", 40.0, 10.0, 10),
    ];
    let found = check(&feeders, None, &Rules::default());
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        (found[0].severity, found[0].feeder.as_str()),
        (Severity::Error, "F2")
    );
    assert!(found[0].message.contains("'F1'"));
    // a feeder that is switched off is not in the way, and a gap of 0 does not check
    let mut off = feeders.clone();
    off[1].enabled = false;
    assert!(check(&off, None, &Rules::default()).is_empty());
    let none = Rules {
        min_gap_mm: 0.0,
        ..Rules::default()
    };
    assert!(check(&feeders, None, &none).is_empty());
    // the same place, whatever the order of the table
    let swapped = [feeders[1].clone(), feeders[0].clone(), feeders[2].clone()];
    let found = check(&swapped, None, &Rules::default());
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].feeder, "F1",
        "the feeder that comes later in the table is the one named"
    );
}

#[test]
fn many_feeders_are_checked_without_comparing_every_pair() {
    // a row of 5000 feeders 5 mm apart does not clash; moving one onto the next does, once
    let mut feeders: Vec<FeederSpec> = (0..5000)
        .map(|i| tape(&format!("F{i}"), f64::from(i) * 5.0 - 12_000.0, 0.0, 10))
        .collect();
    let rules = Rules {
        x_min: -20_000.0,
        x_max: 20_000.0,
        ..Rules::default()
    };
    assert!(check(&feeders, None, &rules).is_empty());
    feeders[2500].base.x = feeders[2501].base.x + 1.0;
    let found = check(&feeders, None, &rules);
    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn the_text_puts_the_errors_first_and_counts_them() {
    let mut a = tape("F1", 0.0, 0.0, 0);
    let b = tape("F2", 1.0, 0.0, 10);
    a.slot = Some("S".into());
    let found = check(&[a, b], None, &Rules::default());
    let (errors, warnings) = counts(&found);
    assert_eq!((errors, warnings), (1, 1), "{:?}", messages(&found));
    let t = text(&found);
    let lines: Vec<&str> = t.lines().collect();
    assert!(lines[0].starts_with("error: feeder F2:"), "{t}");
    assert!(lines[1].starts_with("warning: feeder F1:"), "{t}");
    assert_eq!(lines[2], "1 errors, 1 warnings");
}

#[test]
fn the_settings_of_the_library_are_in_the_example_with_their_defaults_and_ranges() {
    for k in ["feeder_pitch_step_mm", "feeder_min_gap_mm"] {
        assert!(KEYS.contains(&k), "{k}");
        assert!(MACHINE.contains(&format!("\n{k} = ")), "{k}");
    }
    let e = MachineConfig::parse(MACHINE).unwrap();
    let d = MachineConfig::default();
    assert_eq!((e.feeder_pitch_step_mm, e.feeder_min_gap_mm), (2.0, 3.0));
    assert_eq!((d.feeder_pitch_step_mm, d.feeder_min_gap_mm), (2.0, 3.0));
    let r = Rules::default();
    assert_eq!((r.pitch_step_mm, r.min_gap_mm), (2.0, 3.0));
    for (key, ok, bad) in [
        ("feeder_pitch_step_mm", ["0", "100"], ["-1", "100.5"]),
        ("feeder_min_gap_mm", ["0", "1000"], ["-1", "1001"]),
    ] {
        for v in ok {
            assert!(
                MachineConfig::parse(&format!("{key} = {v}")).is_ok(),
                "{key} {v}"
            );
        }
        for v in bad {
            assert_eq!(
                MachineConfig::parse(&format!("{key} = {v}"))
                    .unwrap_err()
                    .code,
                Code::ConfigOutOfRange,
                "{key} {v}"
            );
        }
    }
}
