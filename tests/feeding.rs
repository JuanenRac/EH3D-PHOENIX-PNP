//! Feeders with a priority, trays and strips, and the choice of the feeder for a part.

use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::feeders::Feeder;
use eh3d_phoenix_pnp::feeding::{assign, choose, from_csv, FeederKind, FeederSpec};
use eh3d_phoenix_pnp::parts::{Part, Side};
use std::collections::BTreeMap;

struct Rng(u64);

impl Rng {
    fn bits(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.bits() % n as u64) as usize
    }

    fn between(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * ((self.bits() >> 11) as f64 / (1u64 << 53) as f64)
    }

    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }
}

fn part(r: &str, v: &str, p: &str, x: f64, y: f64) -> Part {
    Part {
        reference: r.into(),
        value: v.into(),
        package: p.into(),
        x,
        y,
        rotation: 0.0,
        side: Side::Top,
    }
}

fn tape(id: &str, v: &str, p: &str, x: f64, y: f64, count: u32, priority: i32) -> FeederSpec {
    let mut f = FeederSpec::from_basic(Feeder {
        id: id.into(),
        value: v.into(),
        package: p.into(),
        x,
        y,
        count,
    });
    f.priority = priority;
    f
}

const ALL_COLUMNS: &str = "id,value,package,x,y,count,priority,kind,cols,rows,pitch_x,pitch_y,pitch,rotation,enabled,warn_below,slot\n\
F1,10k,R_0603,-20,10,100,high,tape,,,,,,90,yes,10,A1\n\
F2,IC,QFN,0,0,5,,tray,3,2,10,12,,0,,,\n\
F3,LED,L,5,5,10,low,strip,,,,,2,0,no,,\n";

#[test]
fn the_feeders_file_reads_the_optional_columns() {
    let f = from_csv(ALL_COLUMNS).unwrap();
    assert_eq!(f.len(), 3);
    assert_eq!(f[0].priority, 1);
    assert_eq!(f[0].kind, FeederKind::Tape);
    assert_eq!(f[0].tape_rotation, 90.0);
    assert_eq!(f[0].warn_below, 10);
    assert_eq!(f[0].slot.as_deref(), Some("A1"));
    assert!(f[0].enabled);
    assert_eq!(f[1].priority, 0);
    assert_eq!(
        f[1].kind,
        FeederKind::Tray {
            cols: 3,
            rows: 2,
            pitch_x: 10.0,
            pitch_y: 12.0,
            rotation: 0.0
        }
    );
    assert_eq!(f[1].capacity(), 6);
    assert_eq!(f[1].slot, None);
    assert_eq!(f[2].priority, -1);
    assert_eq!(
        f[2].kind,
        FeederKind::Strip {
            pitch: 2.0,
            rotation: 0.0
        }
    );
    assert!(!f[2].enabled);
}

#[test]
fn the_six_columns_of_before_still_make_normal_tape_feeders() {
    let f = from_csv("id,value,package,x,y,count\nF1,10k,R_0603,-20,10,100\n").unwrap();
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].priority, 0);
    assert_eq!(f[0].kind, FeederKind::Tape);
    assert!(f[0].enabled);
    assert_eq!(f[0].base.count, 100);
    // the columns may come in any order, with a semicolon and a byte order mark
    let g = from_csv("\u{feff}count;y;x;package;value;id\n7;2;1;P;V;Z\n").unwrap();
    assert_eq!((g[0].base.x, g[0].base.y, g[0].base.count), (1.0, 2.0, 7));
}

#[test]
fn a_bad_feeders_file_is_refused_with_its_code() {
    let bad = [
        "",
        "id,value,package,x,y\nF,v,p,1,2\n",
        "id,value,package,x,y,count,colour\nF,v,p,1,2,3,red\n",
        "id,value,package,x,y,count,count\nF,v,p,1,2,3,3\n",
        "id,value,package,x,y,count\nF,v,p,1,2,3\nF,w,q,1,2,3\n",
        "id,value,package,x,y,count\n,v,p,1,2,3\n",
        "id,value,package,x,y,count\nF,v,p,1,2,-3\n",
        "id,value,package,x,y,count\nF,v,p,1,2,1.5\n",
        "id,value,package,x,y,count\nF,v,p,NaN,2,3\n",
        "id,value,package,x,y,count\nF,v,p,1,2\n",
        "id,value,package,x,y,count,priority\nF,v,p,1,2,3,urgent\n",
        "id,value,package,x,y,count,enabled\nF,v,p,1,2,3,maybe\n",
        "id,value,package,x,y,count,kind\nF,v,p,1,2,3,reel\n",
        "id,value,package,x,y,count,kind,cols,rows,pitch_x,pitch_y\nF,v,p,1,2,3,tray,,2,1,1\n",
        "id,value,package,x,y,count,kind,cols,rows,pitch_x,pitch_y\nF,v,p,1,2,3,tray,1,2,1,0\n",
        "id,value,package,x,y,count,kind,cols,rows,pitch_x,pitch_y\nF,v,p,1,2,9,tray,2,2,1,1\n",
        "id,value,package,x,y,count,kind,pitch\nF,v,p,1,2,3,strip,0\n",
        "id,value,package,x,y,count,kind\nF,v,p,1,2,3,strip\n",
    ];
    for text in bad {
        let e = from_csv(text).unwrap_err();
        assert_eq!(e.code, Code::FeederBadRow, "{text:?}: {}", e.detail);
    }
    let e = from_csv("id,value,package,x,y,count\nA,v,p,1,2,3\nB,v,p,1,x,3\n").unwrap_err();
    assert!(e.detail.contains("row 3"), "{}", e.detail);
}

#[test]
fn a_strip_and_a_tray_say_where_each_next_part_is() {
    let f = from_csv(ALL_COLUMNS).unwrap();
    // the tape: always the same place
    assert_eq!(f[0].pick_point(0), (-20.0, 10.0));
    assert_eq!(f[0].pick_point(50), (-20.0, 10.0));
    // the tray holds 5 of 6 cells: the first cell is gone, so the next one is the second, then the third, then the first of the second row
    assert_eq!(f[1].pick_point(0), (10.0, 0.0));
    assert_eq!(f[1].pick_point(1), (20.0, 0.0));
    assert_eq!(f[1].pick_point(2), (0.0, 12.0));
    assert_eq!(f[1].pick_point(4), (20.0, 12.0));
    // the strip goes along its line
    assert_eq!(f[2].pick_point(0), (5.0, 5.0));
    assert_eq!(f[2].pick_point(3), (11.0, 5.0));
}

#[test]
fn a_turned_strip_and_a_turned_tray_turn_their_lines() {
    let f = from_csv(
        "id,value,package,x,y,count,kind,pitch,cols,rows,pitch_x,pitch_y,rotation\n\
         S,v,p,0,0,10,strip,2,,,,,90\n\
         T,v,p,0,0,6,tray,,3,2,10,12,90\n",
    )
    .unwrap();
    let s = f[0].pick_point(3);
    assert!(s.0.abs() < 1e-9 && (s.1 - 6.0).abs() < 1e-9, "{s:?}");
    // the cell in column 1, row 1 is at (10, 12) before the turn and at (-12, 10) after it
    let t = f[1].pick_point(4);
    assert!(
        (t.0 + 12.0).abs() < 1e-9 && (t.1 - 10.0).abs() < 1e-9,
        "{t:?}"
    );
}

#[test]
fn the_feeder_of_higher_priority_is_used_up_first_and_then_the_other() {
    let feeders = vec![
        tape("LOW", "10k", "R", 0.0, 0.0, 5, -1),
        tape("HIGH", "10k", "R", 100.0, 0.0, 5, 1),
    ];
    let parts: Vec<Part> = (0..7)
        .map(|i| part(&format!("R{i}"), "10k", "R", f64::from(i), 0.0))
        .collect();
    let picks = assign(&parts, &feeders, None);
    assert!(picks.short.is_empty() && picks.without_feeder.is_empty());
    let from_high = picks.picks.iter().filter(|p| p.feeder == 1).count();
    let from_low = picks.picks.iter().filter(|p| p.feeder == 0).count();
    assert_eq!((from_high, from_low), (5, 2));
    // the parts that are taken first, in the order of the references, are the ones that use the high priority feeder
    let first_five: Vec<usize> = (0..5).collect();
    assert!(first_five
        .iter()
        .all(|i| picks.picks.iter().any(|p| p.part == *i && p.feeder == 1)));
}

#[test]
fn with_the_same_priority_the_feeder_nearest_to_the_part_wins_and_then_the_lower_id() {
    let feeders = vec![
        tape("B", "10k", "R", 0.0, 0.0, 10, 0),
        tape("A", "10k", "R", 100.0, 0.0, 10, 0),
    ];
    let parts = vec![
        part("R1", "10k", "R", 90.0, 0.0),
        part("R2", "10k", "R", 5.0, 0.0),
    ];
    let picks = assign(&parts, &feeders, None);
    let of = |r: usize| picks.picks.iter().find(|p| p.part == r).unwrap().feeder;
    assert_eq!(of(0), 1, "R1 is near the feeder A");
    assert_eq!(of(1), 0, "R2 is near the feeder B");
    // equally near: the lower id
    let tie = vec![
        tape("Z", "10k", "R", -10.0, 0.0, 10, 0),
        tape("Y", "10k", "R", 10.0, 0.0, 10, 0),
    ];
    let p = assign(&[part("R1", "10k", "R", 0.0, 0.0)], &tie, None);
    assert_eq!(p.picks[0].feeder, 1, "Y comes before Z");
    assert_eq!(
        choose(&tie, &[10, 10], &[0, 0], "10k|r", (0.0, 0.0)),
        Some(1)
    );
    assert_eq!(choose(&tie, &[0, 0], &[0, 0], "10k|r", (0.0, 0.0)), None);
    assert_eq!(choose(&tie, &[10, 10], &[0, 0], "other", (0.0, 0.0)), None);
}

#[test]
fn a_switched_off_feeder_is_not_used_and_a_kind_without_a_working_feeder_is_short() {
    let mut off = tape("OFF", "10k", "R", 0.0, 0.0, 10, 0);
    off.enabled = false;
    let on = tape("ON", "10k", "R", 50.0, 0.0, 1, 0);
    let parts = vec![
        part("R1", "10k", "R", 0.0, 0.0),
        part("R2", "10k", "R", 1.0, 0.0),
        part("C1", "100n", "C", 2.0, 0.0),
    ];
    let picks = assign(&parts, &[off, on], None);
    assert_eq!(picks.picks.len(), 1);
    assert_eq!(picks.picks[0].feeder, 1);
    assert_eq!(picks.short, vec![("ON".to_string(), 1)]);
    assert_eq!(picks.without_feeder, vec!["C1".to_string()]);
}

#[test]
fn the_parts_of_the_other_side_neither_take_a_feeder_nor_use_it_up() {
    let feeders = vec![tape("A", "10k", "R", 0.0, 0.0, 1, 0)];
    let mut bottom = part("R2", "10k", "R", 0.0, 0.0);
    bottom.side = Side::Bottom;
    let parts = vec![bottom, part("R1", "10k", "R", 0.0, 0.0)];
    let top = assign(&parts, &feeders, Some(Side::Top));
    assert_eq!(top.picks.len(), 1);
    assert_eq!(top.picks[0].part, 1);
    assert!(top.short.is_empty());
    let bottom_only = assign(&parts, &feeders, Some(Side::Bottom));
    assert_eq!(bottom_only.picks[0].part, 0);
}

#[test]
fn a_feeder_that_will_be_nearly_empty_is_warned_about() {
    let mut f = tape("A", "10k", "R", 0.0, 0.0, 5, 0);
    f.warn_below = 3;
    let mut g = tape("B", "100n", "C", 0.0, 0.0, 5, 0);
    g.warn_below = 3;
    let parts: Vec<Part> = (0..3)
        .map(|i| part(&format!("R{i}"), "10k", "R", 0.0, 0.0))
        .collect();
    let picks = assign(&parts, &[f, g], None);
    assert_eq!(picks.low, vec![("A".to_string(), 2)]);
}

#[test]
fn the_choice_of_feeders_does_not_depend_on_the_order_of_the_files() {
    let mut rng = Rng(5);
    let kinds = [("10k", "R"), ("100n", "C"), ("MCU", "Q")];
    for round in 0..40 {
        let parts: Vec<Part> = (0..30)
            .map(|i| {
                let (v, p) = kinds[rng.below(3)];
                part(
                    &format!("P{i:02}"),
                    v,
                    p,
                    rng.between(0.0, 100.0),
                    rng.between(0.0, 100.0),
                )
            })
            .collect();
        let mut feeders = Vec::new();
        for (n, (v, p)) in kinds.iter().enumerate() {
            for k in 0..2 {
                let mut f = tape(
                    &format!("F{n}{k}"),
                    v,
                    p,
                    rng.between(-50.0, 0.0),
                    rng.between(0.0, 100.0),
                    4 + rng.below(8) as u32,
                    rng.below(3) as i32 - 1,
                );
                if rng.below(4) == 0 {
                    f.kind = FeederKind::Strip {
                        pitch: 4.0,
                        rotation: 0.0,
                    };
                }
                feeders.push(f);
            }
        }
        let describe = |parts: &[Part], feeders: &[FeederSpec]| {
            let picks = assign(parts, feeders, None);
            let mut by_ref: BTreeMap<String, (String, i64, i64)> = BTreeMap::new();
            for p in &picks.picks {
                by_ref.insert(
                    parts[p.part].reference.clone(),
                    (
                        feeders[p.feeder].base.id.clone(),
                        (p.at.0 * 1000.0).round() as i64,
                        (p.at.1 * 1000.0).round() as i64,
                    ),
                );
            }
            (by_ref, picks.without_feeder, picks.short)
        };
        let reference = describe(&parts, &feeders);
        let (mut p2, mut f2) = (parts.clone(), feeders.clone());
        rng.shuffle(&mut p2);
        rng.shuffle(&mut f2);
        assert_eq!(reference, describe(&p2, &f2), "round {round}");
    }
}

#[test]
fn every_part_gets_at_most_one_feeder_and_no_feeder_gives_more_than_it_holds() {
    let mut rng = Rng(9);
    for _ in 0..30 {
        let parts: Vec<Part> = (0..25)
            .map(|i| part(&format!("P{i:02}"), "10k", "R", rng.between(0.0, 50.0), 0.0))
            .collect();
        let feeders = vec![
            tape("A", "10k", "R", 0.0, 0.0, rng.below(15) as u32, 0),
            tape("B", "10k", "R", 5.0, 0.0, rng.below(15) as u32, 1),
        ];
        let picks = assign(&parts, &feeders, None);
        let mut seen = std::collections::HashSet::new();
        for p in &picks.picks {
            assert!(seen.insert(p.part), "a part has two feeders");
        }
        for (n, f) in feeders.iter().enumerate() {
            let used = picks.picks.iter().filter(|p| p.feeder == n).count() as u32;
            assert!(used <= f.base.count);
        }
        let short: u32 = picks.short.iter().map(|s| s.1).sum();
        assert_eq!(picks.picks.len() as u32 + short, parts.len() as u32);
    }
}
