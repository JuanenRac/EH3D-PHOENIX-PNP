//! Chance on a fixed seed for the readers and writers of the files for assembly: every reader is given the example files damaged in many ways and must answer with a value or an error, never a
//! panic; and the properties they promise (the order of designators is a total order, a list written as ranges is read back, a placement list is read back, a name of the convention is
//! written the same after being read, a search gives only names of the library). The numbers are the same on every run: the seed is in the message of a failure.

use eh3d_phoenix_pnp::asmimport;
use eh3d_phoenix_pnp::busy::{analyse, read_journal};
use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::cplconv;
use eh3d_phoenix_pnp::designators::{compress, natural_cmp};
use eh3d_phoenix_pnp::dfm::{check, parse_features, Limits};
use eh3d_phoenix_pnp::fpsearch::{key, search};
use eh3d_phoenix_pnp::import::{expand_references, RefItem};
use eh3d_phoenix_pnp::ipcname::{normalize, parse};
use eh3d_phoenix_pnp::jlcpcb::{self, Options};
use eh3d_phoenix_pnp::parts::{Part, Side};
use eh3d_phoenix_pnp::stencil;
use eh3d_phoenix_pnp::tower;
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn example(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join(name),
    )
    .unwrap()
}

const NASTY: [&str; 16] = [
    "",
    ",",
    "\"",
    "\u{0}",
    "\u{feff}",
    "1e999",
    "-1e999",
    "NaN",
    "inf",
    "99999999999999999999999999",
    "R1-R999999999",
    "\r\n",
    ";;;;",
    "é漢字",
    "X",
    "-",
];

/// The text damaged in one way: a character taken out, a nasty word put in, a line repeated or dropped, a cut, or the characters of a line swapped.
fn damage(text: &str, rng: &mut Rng) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return NASTY[rng.below(NASTY.len())].to_string();
    }
    let at = rng.below(chars.len());
    match rng.below(6) {
        0 => chars
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != at)
            .map(|(_, c)| *c)
            .collect(),
        1 => {
            let mut s: String = chars[..at].iter().collect();
            s.push_str(NASTY[rng.below(NASTY.len())]);
            s.extend(&chars[at..]);
            s
        }
        2 => chars[..at].iter().collect(),
        3 => {
            let lines: Vec<&str> = text.lines().collect();
            let k = rng.below(lines.len().max(1));
            let mut v = lines.clone();
            v.insert(k, lines.get(k).copied().unwrap_or(""));
            v.join("\n")
        }
        4 => {
            let lines: Vec<&str> = text.lines().collect();
            let k = rng.below(lines.len().max(1));
            lines
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != k)
                .map(|(_, l)| *l)
                .collect::<Vec<_>>()
                .join("\n")
        }
        _ => {
            let mut v = chars.clone();
            let b = rng.below(v.len());
            v.swap(at, b);
            v.into_iter().collect()
        }
    }
}

fn survives<T>(what: &str, seed: u64, f: impl FnOnce() -> T) {
    assert!(
        catch_unwind(AssertUnwindSafe(f)).is_ok(),
        "{what} panicked on seed {seed:#x}"
    );
}

const JOURNAL: &str = "step,at_s,took_s,action,reference,feeder,nozzle,result,status\n\
1,0.000,1.000,pick,R1,F1,1,ok,pending\n\
2,1.000,2.000,place,R1,F1,1,ok,placed\n\
3,9.000,1.000,pick,R2,F1,1,ok,pending\n\
4,10.000,2.000,place,R2,F1,1,PHOENIX-PNP-024,pending\n";

const FEATURES: &str = "kind,id,x_mm,y_mm,w_mm,h_mm,side\n\
outline,board,0,0,100,80,\n\
pad,U1,10,10,0.3,0.8,top\n\
pad,U1,10.5,10,0.3,0.8,top\n\
pad,H1,60,40,3,3,top\n\
hole,via,60,40,0.3,0.3,top\n\
fiducial,F1,3,3,1,1,\n\
tooling,T1,10,2,2,2,\n\
tab,left,0,0,3,80,\n";

#[test]
fn every_reader_is_given_the_example_files_damaged_and_never_panics() {
    let inputs: Vec<String> = [
        example("altium_pnp.csv"),
        example("centroid.csv"),
        example("board.pos"),
        example("board.mnt"),
        example("board_dnp.csv"),
        JOURNAL.to_string(),
        FEATURES.to_string(),
        "Designator,Mid X,Mid Y,Layer,Rotation\nR1,1mm,2mm,Top,0\n".to_string(),
    ]
    .to_vec();
    let mut rng = Rng(0xFAB_F022);
    let catalog = Catalog::new();
    let limits = Limits::from_config(&MachineConfig::default());
    let settings = stencil::Settings::from_config(&MachineConfig::default());
    for round in 0..400u64 {
        let base = &inputs[rng.below(inputs.len())];
        let mut text = damage(base, &mut rng);
        if rng.below(3) == 0 {
            text = damage(&text, &mut rng);
        }
        survives("cplconv::read", round, || {
            let _ = cplconv::read(&text, None);
            let _ = cplconv::detect(&text);
        });
        survives("asmimport", round, || {
            let _ = asmimport::parse(&text);
        });
        survives("check_cpl", round, || {
            let _ = jlcpcb::check_cpl(&text);
        });
        survives("features", round, || {
            if let Ok(f) = parse_features(&text) {
                let r = check(&[], &f, &catalog, &limits);
                let _ = r.text(eh3d_phoenix_pnp::error::Lang::Pt);
                let _ = r.csv(eh3d_phoenix_pnp::error::Lang::Ja);
                let s = stencil::make(&f, &settings);
                let _ = s.text(eh3d_phoenix_pnp::error::Lang::De);
                let _ = s.csv();
            }
        });
        survives("journal", round, || {
            if let Ok(steps) = read_journal(&text) {
                let _ = analyse(&steps, 3).text();
                let _ = tower::timeline(&steps, 2.0, 3.0);
            }
        });
        survives("names", round, || {
            let _ = parse(&text);
            let _ = key(&text);
            let _ = search(&[text.clone(), "R_0603".into()], &text, 2, 5);
        });
        survives("designators", round, || {
            let _ = expand_references(&text);
        });
    }
}

#[test]
fn the_order_of_designators_is_a_total_order_on_names_made_by_chance() {
    let mut rng = Rng(0xD351);
    let letters = ["R", "C", "U", "FID", "TP", "", "r"];
    let names: Vec<String> = (0..60)
        .map(|_| {
            let digits = rng.below(4) + 1;
            let lead = if rng.below(4) == 0 { "0" } else { "" };
            let tail = if rng.below(5) == 0 { "A" } else { "" };
            format!(
                "{}{}{}{}",
                letters[rng.below(letters.len())],
                lead,
                (0..digits)
                    .map(|_| char::from(b'0' + rng.below(10) as u8))
                    .collect::<String>(),
                tail
            )
        })
        .collect();
    for a in &names {
        assert_eq!(natural_cmp(a, a), Ordering::Equal);
        for b in &names {
            assert_eq!(natural_cmp(a, b), natural_cmp(b, a).reverse(), "{a} {b}");
            if natural_cmp(a, b) == Ordering::Equal {
                assert_eq!(a, b);
            }
            for c in &names {
                if natural_cmp(a, b) != Ordering::Greater && natural_cmp(b, c) != Ordering::Greater
                {
                    assert_ne!(natural_cmp(a, c), Ordering::Greater, "{a} {b} {c}");
                }
            }
        }
    }
}

#[test]
fn a_list_written_as_ranges_is_read_back_whatever_its_order() {
    let mut rng = Rng(0xAAA);
    for round in 0..300 {
        let prefixes = ["R", "C", "L", "D", "Q"];
        let mut set: BTreeSet<String> = BTreeSet::new();
        for _ in 0..1 + rng.below(40) {
            set.insert(format!("{}{}", prefixes[rng.below(5)], 1 + rng.below(30)));
        }
        let mut list: Vec<String> = set.iter().cloned().collect();
        // shuffle
        for i in (1..list.len()).rev() {
            list.swap(i, rng.below(i + 1));
        }
        let text = compress(&list, 3);
        let mut back: BTreeSet<String> = BTreeSet::new();
        for item in
            expand_references(&text).unwrap_or_else(|e| panic!("round {round}: {text}: {e}"))
        {
            match item {
                RefItem::Single(s) => {
                    back.insert(s);
                }
                RefItem::Range(v) => back.extend(v),
            }
        }
        assert_eq!(back, set, "round {round}: {text}");
        let mut again = list.clone();
        again.reverse();
        assert_eq!(
            compress(&again, 3),
            text,
            "the order of the list changed the text"
        );
    }
}

#[test]
fn a_placement_list_made_by_chance_is_read_back_with_every_part_in_its_place() {
    let mut rng = Rng(0x91C0);
    for round in 0..200 {
        let parts: Vec<Part> = (0..1 + rng.below(25))
            .map(|i| Part {
                reference: format!("{}{}", ["R", "C", "U", "J"][rng.below(4)], i + 1),
                value: ["10k", "100n", "MCU", ""][rng.below(4)].into(),
                package: ["R_0603", "C_0603", "QFN-32", "PinHeader_1x04"][rng.below(4)].into(),
                x: rng.unit() * 600.0 - 300.0,
                y: rng.unit() * 600.0 - 300.0,
                rotation: rng.unit() * 359.0,
                side: if rng.below(2) == 0 {
                    Side::Top
                } else {
                    Side::Bottom
                },
            })
            .collect();
        let (mounted, off): (Vec<Part>, Vec<Part>) = parts
            .iter()
            .cloned()
            .partition(|p| p.reference.len() % 2 == 0);
        let text = jlcpcb::cpl(&mounted, &off, &Options::default());
        assert!(jlcpcb::check_cpl(&text).is_empty(), "round {round}");
        let read = cplconv::read(&text, None).unwrap_or_else(|e| panic!("round {round}: {e}"));
        assert_eq!(
            (read.parts.len(), read.dnp.len()),
            (mounted.len(), off.len()),
            "round {round}"
        );
        for p in &parts {
            let q = read
                .parts
                .iter()
                .chain(read.dnp.iter())
                .find(|q| q.reference == p.reference)
                .unwrap();
            assert!(
                (q.x - p.x).abs() < 1e-3 && (q.y - p.y).abs() < 1e-3,
                "round {round} {}",
                p.reference
            );
            assert_eq!(q.side, p.side);
            let turn = (q.rotation - p.rotation).abs();
            assert!(
                turn < 0.01 || (360.0 - turn) < 0.01,
                "round {round} {}",
                p.reference
            );
        }
    }
}

#[test]
fn a_name_of_the_convention_is_written_the_same_after_it_is_read() {
    let mut rng = Rng(0x7351B);
    for round in 0..300 {
        let n = |rng: &mut Rng, hi: usize| 1 + rng.below(hi);
        let d = ['L', 'N', 'M'][rng.below(3)];
        let name = match rng.below(3) {
            0 => format!(
                "RESC{:02}{:02}X{}{d}",
                n(&mut rng, 98),
                n(&mut rng, 98),
                n(&mut rng, 300)
            ),
            1 => format!(
                "SOIC{}P{}X{}-{}{d}",
                n(&mut rng, 300),
                n(&mut rng, 2000),
                n(&mut rng, 400),
                n(&mut rng, 64)
            ),
            _ => format!(
                "QFP{}P{}X{}X{}-{}{d}",
                n(&mut rng, 200),
                n(&mut rng, 4000),
                n(&mut rng, 4000),
                n(&mut rng, 400),
                n(&mut rng, 300)
            ),
        };
        let first = normalize(&name, None).unwrap_or_else(|e| panic!("round {round}: {name}: {e}"));
        assert_eq!(first, name, "round {round}");
        assert_eq!(normalize(&first, None).unwrap(), first);
        assert_eq!(parse(&first).unwrap().density.letter(), d);
    }
}

#[test]
fn a_search_gives_only_names_of_the_library_once_each_and_never_more_than_the_limit() {
    let mut rng = Rng(0x5EA);
    let pool = [
        "R_0603",
        "R_0805",
        "C_0603",
        "RESC1608X55N",
        "SOIC-8",
        "SOIC-14",
        "QFN-32",
        "QFN-48",
        "LED_0603",
        "TQFP-44",
    ];
    for round in 0..200 {
        let library: Vec<String> = (0..rng.below(25))
            .map(|_| pool[rng.below(pool.len())].to_string())
            .collect();
        let query = pool[rng.below(pool.len())];
        let limit = 1 + rng.below(6);
        let hits = search(&library, query, rng.below(4), limit);
        assert!(hits.len() <= limit, "round {round}");
        let mut seen = BTreeSet::new();
        for h in &hits {
            assert!(library.contains(&h.name), "round {round}: {}", h.name);
            assert!(
                seen.insert(h.name.clone()),
                "round {round}: {} twice",
                h.name
            );
        }
        if library.iter().any(|n| n.eq_ignore_ascii_case(query)) {
            assert_eq!(hits[0].how.name(), "exact", "round {round}");
        }
    }
}
