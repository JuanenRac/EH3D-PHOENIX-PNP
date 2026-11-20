//! Chance on a fixed seed for the readers and the figures of the shop floor: the tables of rejects, rules, prices and lots, the signed records, the shift, the bodies of the parts and the
//! costs. The seed is fixed, so a failure repeats; nothing here uses a clock or the network.

use eh3d_phoenix_pnp::bom::Bom;
use eh3d_phoenix_pnp::bomcost;
use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::clearance;
use eh3d_phoenix_pnp::footcheck;
use eh3d_phoenix_pnp::headsplit;
use eh3d_phoenix_pnp::lotage;
use eh3d_phoenix_pnp::lottrace;
use eh3d_phoenix_pnp::parts::{Part, Side};
use eh3d_phoenix_pnp::rejects;
use eh3d_phoenix_pnp::retrypolicy;
use eh3d_phoenix_pnp::serialsig;
use eh3d_phoenix_pnp::sha256;
use eh3d_phoenix_pnp::shiftplan;
use std::path::PathBuf;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn unit(&mut self) -> f64 {
        (self.next() % 1_000_000) as f64 / 1_000_000.0
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

/// The text damaged in many ways: a character changed, taken out, doubled, a line cut, the text cut short, a special character put in.
fn damaged(rng: &mut Rng, text: &str) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    let specials = [
        ',', ';', '"', '\n', '\r', '\u{0}', '\u{feff}', 'é', '日', '-', '9', ' ',
    ];
    for _ in 0..1 + rng.below(4) {
        let at = rng.below(chars.len() as u64) as usize;
        match rng.below(6) {
            0 => chars[at] = specials[rng.below(specials.len() as u64) as usize],
            1 => {
                chars.remove(at);
            }
            2 => chars.insert(at, chars[at]),
            3 => chars.truncate(at),
            4 => chars.insert(at, specials[rng.below(specials.len() as u64) as usize]),
            _ => {
                let end = (at + 1 + rng.below(20) as usize).min(chars.len());
                chars.drain(at..end);
            }
        }
        if chars.is_empty() {
            break;
        }
    }
    chars.into_iter().collect()
}

fn part(r: &str, package: &str, x: f64, y: f64, rot: f64, side: Side) -> Part {
    Part {
        reference: r.into(),
        value: "1k".into(),
        package: package.into(),
        x,
        y,
        rotation: rot,
        side,
    }
}

#[test]
fn no_reader_of_the_shop_floor_panics_on_the_example_files_damaged_by_chance() {
    let mut rng = Rng(0x5eed_0fca_fe00_u64);
    let rejects_text = example("rejects.csv");
    let rules_text = example("retry_rules.csv");
    let prices_text = example("prices.csv");
    let lots_text = example("lots.csv");
    let record = serialsig::sign(
        &serialsig::Record {
            serial: "0042".into(),
            parts: serialsig::fingerprint("a"),
            feeders: serialsig::fingerprint("b"),
            lots: serialsig::NONE.into(),
            prev: serialsig::NONE.into(),
            rows: vec![serialsig::Row {
                reference: "R1".into(),
                feeder: "F1".into(),
                lot: "L1".into(),
            }],
        },
        None,
    )
    .unwrap();
    let (mut ok, mut refused) = (0, 0);
    for _ in 0..400 {
        let mut count = |good: bool| {
            if good {
                ok += 1;
            } else {
                refused += 1;
            }
        };
        count(rejects::parse(&damaged(&mut rng, &rejects_text)).is_ok());
        count(retrypolicy::parse_rules(&damaged(&mut rng, &rules_text)).is_ok());
        count(bomcost::parse_prices(&damaged(&mut rng, &prices_text)).is_ok());
        count(lottrace::parse_lots(&damaged(&mut rng, &lots_text)).is_ok());
        // a record that was damaged is never whole, unless the damage fell where nothing counts
        let d = damaged(&mut rng, &record);
        let verdict = serialsig::verify(&d, None, None);
        if verdict.is_ok() {
            assert_eq!(d.trim_start_matches('\u{feff}'), record, "{d:?}");
        }
        let _ = lotage::parse_week(&damaged(&mut rng, "2411"));
        let _ = footcheck::core(&damaged(&mut rng, "Resistor_SMD:R_0603_1608Metric"));
    }
    // the damage is sometimes refused and sometimes goes through: the test would be empty otherwise
    assert!(ok > 50 && refused > 50, "{ok} {refused}");
}

#[test]
fn a_record_made_of_anything_is_read_back_the_same_and_the_hash_is_the_published_one() {
    let mut rng = Rng(0x0123_4567_89ab_cdef);
    let alphabet: Vec<char> = "abcXYZ019 ,;\"'-_./+:é日ß#".chars().collect();
    // a word has no space at either end: the record refuses one that has
    let word = |rng: &mut Rng, max: u64| -> String {
        let w: String = (0..1 + rng.below(max))
            .map(|_| alphabet[rng.below(alphabet.len() as u64) as usize])
            .collect();
        w.trim().to_string()
    };
    for round in 0..200 {
        let rows: Vec<serialsig::Row> = (0..rng.below(12))
            .map(|i| serialsig::Row {
                reference: format!("{}{i}", word(&mut rng, 5)),
                feeder: word(&mut rng, 6),
                lot: if rng.below(3) == 0 {
                    String::new()
                } else {
                    word(&mut rng, 10)
                },
            })
            .collect();
        let record = serialsig::Record {
            serial: word(&mut rng, 30).trim().to_string() + "x",
            parts: sha256::sha256_hex(&rng.next().to_le_bytes()),
            feeders: sha256::sha256_hex(&rng.next().to_le_bytes()),
            lots: if round % 2 == 0 {
                serialsig::NONE.into()
            } else {
                sha256::sha256_hex(b"lots")
            },
            prev: serialsig::NONE.into(),
            rows,
        };
        let key = (round % 3 == 0).then(|| word(&mut rng, 40).into_bytes());
        let text = serialsig::sign(&record, key.as_deref()).unwrap();
        let back = serialsig::verify(&text, key.as_deref(), None).unwrap();
        assert_eq!(back.record, record, "round {round}");
        // another key, or none, does not pass for the right one
        if key.is_some() {
            assert!(serialsig::verify(&text, Some(b"not it"), None).is_err());
            assert!(serialsig::verify(&text, None, None).is_err());
        }
    }
    // the same bytes cut in different places give different hashes, and one byte more is a different hash
    let data: Vec<u8> = (0..300u32).map(|i| (i * 7 + 3) as u8).collect();
    let mut seen = std::collections::BTreeSet::new();
    for n in 0..=data.len() {
        assert!(seen.insert(sha256::sha256_hex(&data[..n])), "length {n}");
    }
}

#[test]
fn a_longer_shift_never_holds_fewer_boards_and_the_seconds_add_up() {
    let mut rng = Rng(0xfeed_beef_1234);
    for round in 0..150 {
        let reels: Vec<shiftplan::Reel> = (0..1 + rng.below(5))
            .map(|i| {
                let per_board = 1 + rng.below(8) as u32;
                let size = per_board * (1 + rng.below(30) as u32);
                shiftplan::Reel {
                    id: format!("F{i}"),
                    per_board,
                    left: rng.below(size as u64 + 1) as u32,
                    size,
                }
            })
            .collect();
        let board_s = 5.0 + rng.unit() * 120.0;
        let change_s = rng.unit() * 120.0;
        let mut last = 0u64;
        for minutes in [30.0, 60.0, 120.0, 240.0, 480.0] {
            let shift = shiftplan::plan(&shiftplan::Inputs {
                board_s,
                shift_s: minutes * 60.0,
                reel_change_s: change_s,
                boards: None,
                reels: reels.clone(),
            })
            .unwrap();
            assert!(
                shift.boards >= last,
                "round {round}: {} then {last}",
                shift.boards
            );
            last = shift.boards;
            let used = shift.work_s + shift.change_s;
            assert!(used <= minutes * 60.0 + 1e-6, "round {round}");
            assert!(
                (used + shift.left_s - minutes * 60.0).abs() < 1e-6,
                "round {round}"
            );
            assert!((shift.work_s - shift.boards as f64 * board_s).abs() < 1e-6);
            assert!((shift.change_s - shift.changes.len() as f64 * change_s).abs() < 1e-6);
            // the changes come in the order of the boards and of the time
            assert!(shift
                .changes
                .windows(2)
                .all(|w| w[0].board <= w[1].board && w[0].at_s <= w[1].at_s));
            assert!(shift
                .changes
                .iter()
                .all(|c| c.board >= 1 && c.board <= shift.boards + 1));
        }
    }
}

#[test]
fn the_bodies_of_the_parts_are_judged_the_same_whatever_the_order_and_the_place_of_the_board() {
    let mut rng = Rng(0xabad_cafe_0042);
    let mut catalog = Catalog::new();
    catalog
        .add_packages("package,body_long_mm,body_short_mm\nA,2,1\nB,3.2,1.6\nC,5,5\n")
        .unwrap();
    for round in 0..40 {
        let parts: Vec<Part> = (0..50)
            .map(|i| {
                part(
                    &format!("P{i:02}"),
                    ["A", "B", "C"][rng.below(3) as usize],
                    rng.unit() * 40.0,
                    rng.unit() * 30.0,
                    [0.0, 90.0, 45.0, 180.0, 12.5][rng.below(5) as usize],
                    if rng.below(4) == 0 {
                        Side::Bottom
                    } else {
                        Side::Top
                    },
                )
            })
            .collect();
        let base = clearance::check(&parts, &catalog, 0.3);
        let mut reversed = parts.clone();
        reversed.reverse();
        assert_eq!(
            clearance::check(&reversed, &catalog, 0.3).pairs,
            base.pairs,
            "round {round}"
        );
        // the whole board moved by whole millimetres: the same pairs, with the gaps kept to the rounding of the sums
        let moved: Vec<Part> = parts
            .iter()
            .map(|p| Part {
                x: p.x + 100.0,
                y: p.y - 25.0,
                ..p.clone()
            })
            .collect();
        let again = clearance::check(&moved, &catalog, 0.3);
        assert_eq!(again.pairs.len(), base.pairs.len(), "round {round}");
        for (a, b) in again.pairs.iter().zip(&base.pairs) {
            assert!((a.gap_mm - b.gap_mm).abs() < 1e-6);
        }
        // a larger minimum finds at least the same pairs, a smaller one at most
        assert!(clearance::check(&parts, &catalog, 1.0).pairs.len() >= base.pairs.len());
        assert!(clearance::check(&parts, &catalog, 0.0).pairs.len() <= base.pairs.len());
        // the nearest pairs are listed first
        assert!(base.pairs.windows(2).all(|w| w[0].gap_mm <= w[1].gap_mm));
    }
}

#[test]
fn the_cost_of_a_run_is_the_sum_of_its_lines_and_never_less_than_the_cheapest_price_for_what_is_needed(
) {
    let bom = Bom::parse(&example("bom.csv"), 100).unwrap();
    let prices = bomcost::parse_prices(&example("prices.csv")).unwrap();
    let mut rng = Rng(0x7777_1111_9999);
    let mut last_needed = 0;
    for _ in 0..80 {
        let boards = 1 + rng.below(500);
        let spare = rng.unit() * 40.0;
        let cost = bomcost::cost(&bom, &prices, boards, spare).unwrap();
        let sum: f64 = cost.lines.iter().map(|l| l.cost).sum();
        assert!((sum - cost.total).abs() < 1e-6);
        for l in &cost.lines {
            assert!(l.bought >= l.needed);
            assert!(l.needed >= l.per_board * boards, "the spares only add");
            let cheapest = prices.iter().map(|p| p.each).fold(f64::MAX, f64::min);
            assert!(l.cost + 1e-9 >= l.needed as f64 * cheapest.min(l.each));
        }
        // the same run is the same cost
        assert_eq!(cost, bomcost::cost(&bom, &prices, boards, spare).unwrap());
        // more spares never mean fewer pieces
        let more = bomcost::cost(&bom, &prices, boards, spare + 5.0).unwrap();
        for (a, b) in cost.lines.iter().zip(&more.lines) {
            if a.part_number == b.part_number && a.value == b.value {
                assert!(b.needed >= a.needed);
            }
        }
        last_needed = last_needed.max(cost.lines.iter().map(|l| l.needed).max().unwrap_or(0));
    }
    assert!(
        last_needed > 100,
        "the runs were large enough to reach a price break"
    );
}

#[test]
fn the_rejects_add_up_by_reason_by_reference_and_by_package() {
    use eh3d_phoenix_pnp::feeding::{assign, from_csv};
    let parts: Vec<Part> = (0..12)
        .map(|i| {
            part(
                &format!("R{i}"),
                ["0603", "0402", "SOT-23"][i % 3],
                0.0,
                0.0,
                0.0,
                Side::Top,
            )
        })
        .collect();
    let feeders = from_csv("id,value,package,count,x,y\nF1,1k,0603,100,0,0\nF2,1k,0402,100,1,0\nF3,1k,SOT-23,100,2,0\n").unwrap();
    let picks = assign(&parts, &feeders, None);
    let mut rng = Rng(0x1357_9bdf_2468);
    for round in 0..60 {
        let n = rng.below(40);
        let reasons = ["tombstone", "skewed", "missing", "bridge"];
        let table: Vec<rejects::Reject> = (0..n)
            .map(|_| rejects::Reject {
                reference: if rng.below(8) == 0 {
                    "X1".into()
                } else {
                    format!("R{}", rng.below(12))
                },
                reason: reasons[rng.below(4) as usize].into(),
                board: format!("{}", 1 + rng.below(5)),
            })
            .collect();
        let report = rejects::analyse(&table, Some(50), &parts, &feeders, &picks).unwrap();
        assert_eq!(report.total as u64, n);
        assert_eq!(
            report.reasons.iter().map(|r| r.count).sum::<usize>(),
            report.total,
            "round {round}"
        );
        let unknown = table.iter().filter(|r| r.reference == "X1").count();
        assert_eq!(
            report.references.iter().map(|r| r.count).sum::<usize>() + unknown,
            report.total
        );
        assert_eq!(
            report.packages.iter().map(|g| g.rejected).sum::<usize>() + unknown,
            report.total
        );
        assert_eq!(
            report.feeders.iter().map(|g| g.rejected).sum::<usize>() + unknown,
            report.total
        );
        assert!(report.reasons.windows(2).all(|w| w[0].count >= w[1].count));
        if let Some(last) = report.reasons.last() {
            assert!((last.cumulative - 1.0).abs() < 1e-9);
        }
        assert_eq!(report.unknown.is_empty(), unknown == 0);
    }
}

#[test]
fn the_age_of_a_lot_grows_with_the_week_and_the_two_shapes_of_a_code_agree() {
    let mut rng = Rng(0x2468_ace0_1357);
    for _ in 0..300 {
        let (yy, ww) = (rng.below(100), 1 + rng.below(53));
        let short = format!("{yy:02}{ww:02}");
        let long = format!("20{yy:02}{ww:02}");
        let (a, b) = (
            lotage::parse_week(&short).unwrap(),
            lotage::parse_week(&long).unwrap(),
        );
        assert_eq!(a, b);
        let later = lotage::parse_week(&format!("{:02}{:02}", (yy + 1).min(99), ww)).unwrap();
        assert!(later.count() >= a.count());
    }
    let mut times = Vec::new();
    for n in 0..60u64 {
        let w = lotage::Week {
            year: 2020 + (n / 52) as u32,
            week: 1 + (n % 52) as u32,
        };
        times.push(w.count());
    }
    assert!(times.windows(2).all(|p| p[1] == p[0] + 1));
}

#[test]
fn a_head_never_places_a_part_that_another_head_has_and_the_heads_never_beat_the_work_shared() {
    let mut rng = Rng(0x0bad_f00d_5eed);
    let t = headsplit::Timing {
        pick_s: 0.4,
        place_s: 0.7,
    };
    for round in 0..80 {
        let n = rng.below(60) as usize;
        let items: Vec<headsplit::Item> = (0..n)
            .map(|i| headsplit::Item {
                reference: format!("R{i:03}"),
                feeder: format!("F{}", rng.below(1 + (round % 7) as u64)),
                x: rng.unit() * 100.0,
            })
            .collect();
        let heads = 1 + rng.below(8) as usize;
        let single = headsplit::single_head_s(&items, t);
        for o in headsplit::compare(&items, heads, t) {
            assert_eq!(
                o.heads.iter().map(|h| h.parts).sum::<usize>(),
                n,
                "round {round}"
            );
            assert!(o.makespan_s + 1e-9 >= single / heads as f64);
            assert!(o.makespan_s <= single + 1e-9);
            // a way that shares no reel never waits
            if o.shared.is_empty() {
                assert!(o.waited_s() < 1e-9, "round {round}: {:?}", o.way);
            }
        }
    }
}
