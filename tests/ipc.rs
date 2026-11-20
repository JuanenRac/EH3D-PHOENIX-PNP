//! The names of the footprints by the public convention of IPC-7351B, written and read, and the search of a name in a library of packages: the same footprint under the names of KiCad, of the
//! convention and of the EIA codes.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::fpsearch::{distance, key, same_footprint, search, text, How};
use eh3d_phoenix_pnp::ipcname::{
    describe, from_package, normalize, parse, Density, IpcName, Shape,
};
use std::path::PathBuf;

fn chip(family: &str, l: f64, w: f64, h: f64, d: Density) -> IpcName {
    IpcName {
        family: family.into(),
        shape: Shape::Chip {
            length_mm: l,
            width_mm: w,
            height_mm: h,
        },
        density: d,
    }
}

/// A generator of numbers from a seed (the same on every run).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

#[test]
fn a_chip_is_written_with_its_body_in_tenths_and_its_height_in_hundredths() {
    let n = chip("RESC", 1.6, 0.8, 0.55, Density::Nominal);
    assert_eq!(n.name().unwrap(), "RESC1608X55N");
    let n = chip("CAPC", 1.0, 0.5, 0.35, Density::Least);
    assert_eq!(n.name().unwrap(), "CAPC1005X35L");
    let n = chip("LEDC", 3.2, 1.6, 0.7, Density::Most);
    assert_eq!(n.name().unwrap(), "LEDC3216X70M");
}

#[test]
fn a_part_with_leads_on_two_sides_is_written_with_pitch_span_height_and_pins() {
    let n = IpcName {
        family: "SOIC".into(),
        shape: Shape::Dual {
            pitch_mm: 1.27,
            span_mm: 6.0,
            height_mm: 1.75,
            pins: 8,
        },
        density: Density::Nominal,
    };
    assert_eq!(n.name().unwrap(), "SOIC127P600X175-8N");
}

#[test]
fn a_part_with_leads_on_four_sides_is_written_with_the_body_and_the_pins() {
    let n = IpcName {
        family: "QFP".into(),
        shape: Shape::Quad {
            pitch_mm: 0.5,
            length_mm: 10.0,
            width_mm: 10.0,
            height_mm: 1.6,
            pins: 48,
        },
        density: Density::Nominal,
    };
    assert_eq!(n.name().unwrap(), "QFP50P1000X1000X160-48N");
}

#[test]
fn a_name_is_taken_apart_and_written_again_the_same() {
    for name in [
        "RESC1608X55N",
        "CAPC1005X35L",
        "SOIC127P600X175-8N",
        "SOT95P280X145-5N",
        "QFP50P1000X1000X160-48N",
        "QFN50P500X500X80-33M",
    ] {
        let parsed = parse(name).unwrap();
        assert_eq!(parsed.name().unwrap(), name, "{name}");
    }
    let q = parse("QFP50P1000X1000X160-48N").unwrap();
    assert_eq!(q.family, "QFP");
    assert!(matches!(q.shape, Shape::Quad { pins: 48, .. }));
    assert_eq!(
        parse("resc1608x55n").unwrap().name().unwrap(),
        "RESC1608X55N"
    );
}

#[test]
fn the_density_can_be_changed_and_has_three_letters_and_two_older_ones() {
    assert_eq!(
        normalize("RESC1608X55N", Some(Density::Most)).unwrap(),
        "RESC1608X55M"
    );
    assert_eq!(normalize("RESC1608X55N", None).unwrap(), "RESC1608X55N");
    for (text, d) in [
        ("L", Density::Least),
        ("n", Density::Nominal),
        ("most", Density::Most),
        ("A", Density::Most),
        ("b", Density::Nominal),
        ("C", Density::Least),
    ] {
        assert_eq!(Density::parse(text), Some(d), "{text}");
    }
    assert_eq!(Density::parse("X"), None);
    assert_eq!(Density::ALL.map(Density::letter), ['L', 'N', 'M']);
}

#[test]
fn a_name_that_cannot_be_written_or_read_is_an_error_with_the_code_of_a_bad_row() {
    for bad in [
        chip("RESC", 12.0, 0.8, 0.5, Density::Nominal),
        chip("RESC", 1.6, 0.8, f64::NAN, Density::Nominal),
        chip("resc", 1.6, 0.8, 0.5, Density::Nominal),
        chip("R", 1.6, 0.8, 0.5, Density::Nominal),
        IpcName {
            family: "SOIC".into(),
            shape: Shape::Dual {
                pitch_mm: 1.27,
                span_mm: 6.0,
                height_mm: 1.75,
                pins: 0,
            },
            density: Density::Nominal,
        },
    ] {
        assert_eq!(bad.name().unwrap_err().code, Code::CatalogBadRow, "{bad:?}");
    }
    for bad in [
        "",
        "RESC",
        "RESC1608X55",
        "RESC160X55N",
        "1608X55N",
        "SOIC127P600-8N",
        "SOIC127P600X175X1X2-8N",
        "SOIC127P600X175-N",
        "SOIC127P600X175-99999N",
        "SOIC12a7P600X175-8N",
    ] {
        assert_eq!(parse(bad).unwrap_err().code, Code::CatalogBadRow, "{bad:?}");
    }
}

#[test]
fn a_chip_package_of_the_table_is_named_by_the_convention_from_its_size() {
    let n = from_package("R_0603", Density::Nominal).unwrap();
    assert_eq!(n.family, "RESC");
    assert_eq!(n.name().unwrap(), "RESC1608X50N");
    assert_eq!(
        from_package("C0402", Density::Least)
            .unwrap()
            .name()
            .unwrap(),
        "CAPC1005X35L"
    );
    assert_eq!(
        from_package("LED-1206", Density::Most).unwrap().family,
        "LEDC"
    );
    assert_eq!(
        from_package("L_0805", Density::Nominal).unwrap().family,
        "INDC"
    );
    for bad in ["0603", "SOIC-8", "R_9999", "X_0603", "QFN-32"] {
        assert!(from_package(bad, Density::Nominal).is_err(), "{bad}");
    }
}

#[test]
fn a_name_is_described_in_a_line() {
    let line = describe(&parse("SOIC127P600X175-8N").unwrap());
    assert!(line.contains("pitch 1.27 mm") && line.contains("8 pins") && line.contains("nominal"));
    let line = describe(&parse("RESC1608X55L").unwrap());
    assert!(line.contains("1.6 x 0.8") && line.contains("least"));
}

#[test]
fn names_made_on_a_fixed_seed_are_read_back_to_the_same_sizes() {
    let mut rng = Rng(0x1BC_7351);
    for round in 0..500 {
        let d = Density::ALL[rng.below(3) as usize];
        let hundredths = |rng: &mut Rng, lo: u64, hi: u64| (lo + rng.below(hi - lo)) as f64 / 100.0;
        let made = match rng.below(3) {
            0 => IpcName {
                family: ["RESC", "CAPC", "INDC", "LEDC"][rng.below(4) as usize].into(),
                shape: Shape::Chip {
                    length_mm: (1 + rng.below(98)) as f64 / 10.0,
                    width_mm: (1 + rng.below(98)) as f64 / 10.0,
                    height_mm: hundredths(&mut rng, 1, 500),
                },
                density: d,
            },
            1 => IpcName {
                family: ["SOIC", "SOP", "SSOP", "TSSOP", "SOT"][rng.below(5) as usize].into(),
                shape: Shape::Dual {
                    pitch_mm: hundredths(&mut rng, 20, 300),
                    span_mm: hundredths(&mut rng, 100, 2000),
                    height_mm: hundredths(&mut rng, 10, 500),
                    pins: 1 + rng.below(100) as u32,
                },
                density: d,
            },
            _ => IpcName {
                family: ["QFP", "QFN", "DFN", "LQFP"][rng.below(4) as usize].into(),
                shape: Shape::Quad {
                    pitch_mm: hundredths(&mut rng, 20, 200),
                    length_mm: hundredths(&mut rng, 100, 5000),
                    width_mm: hundredths(&mut rng, 100, 5000),
                    height_mm: hundredths(&mut rng, 10, 500),
                    pins: 1 + rng.below(300) as u32,
                },
                density: d,
            },
        };
        let name = made
            .name()
            .unwrap_or_else(|e| panic!("round {round}: {made:?}: {e}"));
        let again = parse(&name).unwrap_or_else(|e| panic!("round {round}: {name}: {e}"));
        assert_eq!(again.name().unwrap(), name, "round {round}");
        assert_eq!(again.density, d);
        assert_eq!(again.family, made.family);
    }
}

// ---- the search

#[test]
fn the_same_footprint_has_one_key_under_the_names_of_kicad_the_convention_and_the_codes() {
    let k = key("Resistor_SMD:R_0603_1608Metric");
    assert_eq!(k, key("R_0603"));
    assert_eq!(k, key("r0603"));
    assert_eq!(k, key("RESC1608X55N"));
    assert_eq!(k, key("RESC1608X50L"));
    assert_ne!(k, key("C_0603"));
    assert_ne!(k, key("R_0805"));
    assert!(same_footprint(&key("0603"), &key("R_0603")));
    assert!(same_footprint(&key("0603"), &key("C_0603")));
    assert!(!same_footprint(&key("R_0603"), &key("C_0603")));
    assert!(!same_footprint(&key("0603"), &key("0805")));
}

#[test]
fn any_other_name_is_its_letters_and_digits_in_capitals() {
    assert_eq!(key("Package_SO:SOIC-8_3.9x4.9mm"), key("SOIC-8_3.9x4.9mm"));
    assert_eq!(key("qfn-32"), key("QFN_32"));
    assert_eq!(key("SOIC127P600X175-8N"), "SOIC 1.27p6.00 8");
}

#[test]
fn the_distance_counts_letters_and_stops_at_the_limit() {
    assert_eq!(distance("kitten", "sitting", 3), Some(3));
    assert_eq!(distance("kitten", "sitting", 2), None);
    assert_eq!(distance("", "abc", 3), Some(3));
    assert_eq!(distance("same", "same", 0), Some(0));
    assert_eq!(distance("a", "abcdefgh", 2), None);
}

fn library() -> Vec<String> {
    [
        "R_0603",
        "R_0805",
        "C_0603",
        "RESC1608X55N",
        "SOIC-8",
        "SOIC-14",
        "QFN-32",
        "LED_0603",
        "Resistor_SMD:R_0603_1608Metric",
        "R_0603",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

#[test]
fn the_search_gives_the_same_text_first_then_the_same_footprint_then_the_words_then_the_near() {
    let hits = search(&library(), "r_0603", 2, 20);
    assert_eq!(hits[0].name, "R_0603");
    assert_eq!(hits[0].how, How::Exact);
    let same: Vec<&str> = hits
        .iter()
        .filter(|h| h.how == How::SameFootprint)
        .map(|h| h.name.as_str())
        .collect();
    assert_eq!(same, ["RESC1608X55N", "Resistor_SMD:R_0603_1608Metric"]);
    // a name is only once in the list
    assert_eq!(hits.iter().filter(|h| h.name == "R_0603").count(), 1);
    let words = search(&library(), "soic", 0, 20);
    assert_eq!(words.len(), 2);
    assert!(words.iter().all(|h| h.how == How::Contains));
    let near = search(&library(), "SOIC-9", 1, 20);
    assert!(near
        .iter()
        .any(|h| h.how == How::Near && h.name == "SOIC-8" && h.distance == 1));
}

#[test]
fn the_limit_and_the_distance_cut_the_list_and_nothing_is_found_for_an_empty_query() {
    assert_eq!(search(&library(), "R_", 0, 2).len(), 2);
    assert!(search(&library(), "   ", 2, 20).is_empty());
    assert!(search(&library(), "ZZZZZZ", 0, 20).is_empty());
    assert!(search(&[], "R_0603", 2, 20).is_empty());
    let lines = text(&search(&library(), "SOIC-9", 1, 20));
    assert!(lines.contains("SOIC-8  (near, 1 letters)"), "{lines}");
}

#[test]
fn a_table_of_packages_is_searched_by_the_names_it_holds() {
    let mut catalog = Catalog::new();
    let packages = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/packages.csv"),
    )
    .unwrap();
    catalog.add_packages(&packages).unwrap();
    let hits = search(
        &catalog.package_names(),
        "Resistor_SMD:R_0603_1608Metric",
        2,
        20,
    );
    assert!(hits
        .iter()
        .any(|h| h.name == "R_0603" && h.how == How::SameFootprint));
}

#[test]
fn the_settings_of_the_names_and_the_search_are_keys_with_ranges() {
    for k in ["ipc_density", "ipc_search_limit", "ipc_search_distance"] {
        assert!(KEYS.contains(&k), "{k}");
    }
    let cfg =
        MachineConfig::parse("ipc_density = M\nipc_search_limit = 5\nipc_search_distance = 0\n")
            .unwrap();
    assert_eq!(cfg.ipc_density, Density::Most);
    assert_eq!((cfg.ipc_search_limit, cfg.ipc_search_distance), (5, 0));
    assert_eq!(MachineConfig::default().ipc_density, Density::Nominal);
    let e = MachineConfig::parse("ipc_search_distance = 9\n").unwrap_err();
    assert_eq!(e.code, Code::ConfigOutOfRange);
    assert!(e.to_string().contains("ipc_search_distance"));
    let e = MachineConfig::parse("ipc_density = Q\n").unwrap_err();
    assert!(e.to_string().contains("ipc_density"));
}
