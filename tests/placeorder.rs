//! The composite order of the places: the list of keys and its directions, how two places compare, the groups, the setting `placement_order` and the plan that follows it.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::placeorder::{compare, groups, parse, text, unit_of, Key, Row, MAX_TERMS};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use std::cmp::Ordering;

fn row(unit: &str, feeder: &str, part: &str, height: f64, area: f64, quantity: usize) -> Row {
    Row {
        unit: unit.into(),
        feeder: feeder.into(),
        part: part.into(),
        height,
        area,
        quantity,
    }
}

#[test]
fn the_list_is_read_with_its_directions_and_written_back() {
    let t = parse("unit, height:asc, Quantity:DESC").unwrap();
    assert_eq!(t.len(), 3);
    assert_eq!((t[0].key, t[0].descending), (Key::Unit, false));
    assert_eq!((t[1].key, t[1].descending), (Key::Height, false));
    assert_eq!((t[2].key, t[2].descending), (Key::Quantity, true));
    assert_eq!(text(&t), "unit:asc,height:asc,quantity:desc");
    assert_eq!(parse(&text(&t)).unwrap(), t);
    // nothing at all is the empty list, which turns the composite order off
    assert!(parse("").unwrap().is_empty());
    assert!(parse(" , ,").unwrap().is_empty());
}

#[test]
fn a_list_that_is_wrong_is_refused_with_why() {
    assert!(parse("colour").unwrap_err().contains("not a key"));
    assert!(parse("unit:up").unwrap_err().contains("asc or desc"));
    assert!(parse("unit,height,unit:desc")
        .unwrap_err()
        .contains("twice"));
    let many = Key::ALL
        .iter()
        .map(|k| k.name())
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(parse(&many).unwrap().len(), MAX_TERMS);
    assert!(parse(&format!("{many},unit")).is_err());
    // cost is not a key: the prices are in a table the planner does not read
    assert!(parse("cost").is_err());
}

#[test]
fn the_first_key_that_tells_two_places_apart_decides_in_its_direction() {
    let t = parse("unit:asc,height:desc,quantity:asc").unwrap();
    let a = row("A", "F1", "R", 0.5, 1.0, 4);
    let b = row("B", "F1", "R", 0.5, 1.0, 4);
    assert_eq!(compare(&t, &a, &b), Ordering::Less);
    let tall = row("A", "F1", "R", 2.0, 1.0, 4);
    assert_eq!(compare(&t, &tall, &a), Ordering::Less, "taller first");
    let few = row("A", "F1", "R", 0.5, 1.0, 2);
    assert_eq!(compare(&t, &few, &a), Ordering::Less, "fewer first");
    assert_eq!(compare(&t, &a, &a.clone()), Ordering::Equal);
    // a key that is not in the list does not count
    let other_feeder = row("A", "F9", "X", 0.5, 9.0, 4);
    assert_eq!(compare(&t, &a, &other_feeder), Ordering::Equal);
    assert_eq!(compare(&[], &a, &b), Ordering::Equal);
}

#[test]
fn places_are_put_in_groups_in_the_order_of_the_list_and_keep_their_own_order_inside() {
    let rows = [
        row("B", "F1", "R", 0.5, 1.0, 1),
        row("A", "F2", "C", 1.0, 2.0, 1),
        row("A", "F1", "R", 0.5, 1.0, 1),
        row("B", "F2", "C", 1.0, 2.0, 1),
        row("A", "F1", "R", 0.5, 1.0, 1),
    ];
    let idx: Vec<usize> = (0..rows.len()).collect();
    let by = |list: &str| groups(&parse(list).unwrap(), &idx, &|k| rows[k].clone());
    assert_eq!(by("unit"), vec![vec![1, 2, 4], vec![0, 3]]);
    assert_eq!(by("unit:desc"), vec![vec![0, 3], vec![1, 2, 4]]);
    assert_eq!(
        by("unit,height:desc"),
        vec![vec![1], vec![2, 4], vec![3], vec![0]]
    );
    // no keys: one group, the places as they came
    assert_eq!(by(""), vec![vec![0, 1, 2, 3, 4]]);
    // the same list on the same places always gives the same groups
    assert_eq!(by("feeder,part"), by("feeder,part"));
    assert_eq!(unit_of("A/R1"), "A");
    assert_eq!(unit_of("B/C/R1"), "B");
    assert_eq!(unit_of("R1"), "");
}

#[test]
fn the_setting_is_known_documented_and_refuses_what_is_not_a_list() {
    assert!(KEYS.contains(&"placement_order"));
    let c = MachineConfig::parse("placement_order = unit, height:desc\n").unwrap();
    assert_eq!(c.placement_order.len(), 2);
    assert!(MachineConfig::default().placement_order.is_empty());
    let e = MachineConfig::parse("placement_order = size\n").unwrap_err();
    assert!(
        e.detail.contains("placement_order") && e.detail.contains("not a key"),
        "{}",
        e.detail
    );
    let example = include_str!("../examples/machine.cfg");
    assert!(example.contains("\nplacement_order ="));
    assert!(MachineConfig::parse(example)
        .unwrap()
        .placement_order
        .is_empty());
}

const PARTS: &str = "ref,val,package,x,y,rot,side\nA/R1,10k,R,10,10,0,top\nA/C1,100n,C,20,10,0,top\nB/R1,10k,R,30,10,0,top\nB/C1,100n,C,40,10,0,top\nA/R2,10k,R,15,20,0,top\n";
const FEEDERS: &str = "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\n";

fn refs(cfg: &str) -> Vec<String> {
    let cfg = MachineConfig::parse(cfg).unwrap();
    let parts = from_csv(PARTS).unwrap();
    let feeders = feeding::from_csv(FEEDERS).unwrap();
    let picks = assign(&parts, &feeders, Some(Side::Top));
    let plan = plan_advanced(
        &parts,
        &feeders,
        &picks,
        &Catalog::new(),
        &cfg,
        &Options::default(),
    )
    .unwrap();
    plan.plan
        .placements
        .iter()
        .map(|p| p.reference.clone())
        .collect()
}

#[test]
fn the_plan_follows_the_list_unit_by_unit_and_a_list_of_nothing_changes_nothing() {
    let by_unit = refs("placement_order = unit\nnozzles = 1\n");
    let units: Vec<&str> = by_unit.iter().map(|r| &r[..1]).collect();
    // all of A, then all of B (a board of a panel is finished before the next is started)
    assert_eq!(units, ["A", "A", "A", "B", "B"], "{by_unit:?}");
    let backwards = refs("placement_order = unit:desc\nnozzles = 1\n");
    let units: Vec<&str> = backwards.iter().map(|r| &r[..1]).collect();
    assert_eq!(units, ["B", "B", "A", "A", "A"], "{backwards:?}");
    // by quantity, the kind that is the most numerous in its board first: the two resistors of A, then the rest
    let by_part = refs("placement_order = quantity:desc\nnozzles = 1\n");
    assert_eq!(by_part[..2], ["A/R1", "A/R2"], "{by_part:?}");
    // the same input gives the same plan every time
    assert_eq!(by_unit, refs("placement_order = unit\nnozzles = 1\n"));
    // empty: the single order decides, as before
    assert_eq!(refs("placement_order =\n"), refs(""));
}
