//! The load sheet and what the plan is turned into: the report, the list of what is missing and the G-code of the movements.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::export::{gcode, missing_csv, report};
use eh3d_phoenix_pnp::feeders::Feeder;
use eh3d_phoenix_pnp::feeding::{assign, from_csv as feeders_from_csv, FeederSpec, Picks};
use eh3d_phoenix_pnp::loadsheet::{assign_slots, needs, slots_from_csv};
use eh3d_phoenix_pnp::parts::{from_csv, Part, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, AdvancedPlan, Options};

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

const SLOTS: &str = "id,x,y,max_width_mm\nS1,0,0,8\nS2,50,50,8\nS3,10,10,12\nS4,200,200,8\n";

fn board() -> Vec<Part> {
    let mut parts = Vec::new();
    for i in 0..4 {
        parts.push(part(&format!("R{i}"), "10k", "R_0603", 10.0, 10.0));
    }
    for i in 0..2 {
        parts.push(part(&format!("C{i}"), "100n", "C_0603", 10.0, 10.0));
    }
    parts.push(part("U1", "MCU", "QFN-32", 10.0, 10.0));
    parts
}

fn catalog() -> Catalog {
    let mut c = Catalog::new();
    c.add_packages("package,tape_mm\nQFN-32,12\n").unwrap();
    c
}

#[test]
fn the_slots_file_is_read_and_checked() {
    let slots = slots_from_csv(SLOTS).unwrap();
    assert_eq!(slots.len(), 4);
    assert_eq!(
        (slots[2].id.as_str(), slots[2].x, slots[2].max_width_mm),
        ("S3", 10.0, 12.0)
    );
    let bad = [
        "",
        "id,x,y\nS1,0,0\n",
        "id,x,y,max_width_mm\nS1,0,0\n",
        "id,x,y,max_width_mm\nS1,0,0,8\nS1,1,1,8\n",
        "id,x,y,max_width_mm\n,0,0,8\n",
        "id,x,y,max_width_mm\nS1,0,0,0\n",
        "id,x,y,max_width_mm\nS1,a,0,8\n",
        "id,x,y,max_width_mm\nS1,0,NaN,8\n",
        "id,x,y,max_width_mm\nS1,0,0,500\n",
    ];
    for text in bad {
        assert_eq!(
            slots_from_csv(text).unwrap_err().code,
            Code::SlotsBad,
            "{text:?}"
        );
    }
    let e = slots_from_csv("id,x,y,max_width_mm\nS1,0,0,8\nS2,x,0,8\n").unwrap_err();
    assert!(e.detail.contains("row 3"), "{}", e.detail);
}

#[test]
fn the_kinds_of_part_are_counted_with_their_tape_and_the_middle_of_the_board() {
    let cfg = MachineConfig::default();
    let (list, middle) = needs(&board(), Some(Side::Top), &catalog(), &cfg);
    let counts: Vec<(String, usize, f64)> = list
        .iter()
        .map(|n| (n.value.clone(), n.count, n.tape_mm))
        .collect();
    assert_eq!(
        counts,
        vec![
            ("10k".to_string(), 4, 8.0),
            ("100n".to_string(), 2, 8.0),
            ("MCU".to_string(), 1, 12.0)
        ]
    );
    assert_eq!(middle, (10.0, 10.0));
    // the other side has no parts
    let (none, centre) = needs(&board(), Some(Side::Bottom), &catalog(), &cfg);
    assert!(none.is_empty());
    assert_eq!(centre, (0.0, 0.0));
}

#[test]
fn the_wide_tape_gets_the_wide_slot_nearest_to_the_board_and_the_others_the_next_nearest() {
    let cfg = MachineConfig::default();
    let (list, middle) = needs(&board(), Some(Side::Top), &catalog(), &cfg);
    let sheet = assign_slots(&list, &slots_from_csv(SLOTS).unwrap(), middle);
    assert!(sheet.unassigned.is_empty());
    let slot_of = |value: &str| {
        sheet
            .rows
            .iter()
            .find(|r| r.need.value == value)
            .map(|r| r.slot.id.clone())
            .unwrap()
    };
    // S3 is the only slot for 12 mm and is the nearest; then S1 (14 mm away) and S2 (57 mm away)
    assert_eq!(slot_of("MCU"), "S3");
    assert_eq!(slot_of("10k"), "S1");
    assert_eq!(slot_of("100n"), "S2");
    // the rows are in the order of the slots
    let ids: Vec<&str> = sheet.rows.iter().map(|r| r.slot.id.as_str()).collect();
    assert_eq!(ids, vec!["S1", "S2", "S3"]);
}

#[test]
fn a_tape_that_fits_in_no_free_slot_is_left_out_and_no_slot_is_given_twice() {
    let cfg = MachineConfig::default();
    let (list, middle) = needs(&board(), Some(Side::Top), &catalog(), &cfg);
    let few = "id,x,y,max_width_mm\nA,0,0,8\nB,5,5,8\n";
    let sheet = assign_slots(&list, &slots_from_csv(few).unwrap(), middle);
    assert_eq!(sheet.rows.len(), 2);
    assert_eq!(sheet.unassigned.len(), 1);
    assert_eq!(
        sheet.unassigned[0].value, "MCU",
        "12 mm does not fit in 8 mm"
    );
    let mut ids: Vec<&str> = sheet.rows.iter().map(|r| r.slot.id.as_str()).collect();
    ids.dedup();
    assert_eq!(ids.len(), 2);
    // no slots at all
    let none = assign_slots(&list, &[], middle);
    assert!(none.rows.is_empty());
    assert_eq!(none.unassigned.len(), 3);
}

#[test]
fn the_sheet_is_printed_for_the_operator_and_read_back_as_feeders() {
    let cfg = MachineConfig::default();
    let mut parts = board();
    parts.push(part("L1", "1u, 5%", "L \"big\"", 10.0, 10.0));
    let (list, middle) = needs(&parts, Some(Side::Top), &catalog(), &cfg);
    let slots = "id,x,y,max_width_mm\nS1,0,0,8\nS2,50,50,8\nS3,10,10,12\nS4,1,1,8\n";
    let sheet = assign_slots(&list, &slots_from_csv(slots).unwrap(), middle);
    assert_eq!(sheet.rows.len(), 4);
    let csv = sheet.to_csv();
    assert!(csv.starts_with("slot,value,package,count,tape_mm,tip,x,y\n"));
    assert!(csv.contains("\"1u, 5%\",\"L \"\"big\"\"\""), "{csv}");
    // the feeders file works as the file of feeders of the planner
    let feeders = feeders_from_csv(&sheet.to_feeders_csv()).unwrap();
    assert_eq!(feeders.len(), 4);
    let picks = assign(&parts, &feeders, Some(Side::Top));
    assert!(picks.without_feeder.is_empty() && picks.short.is_empty());
    assert_eq!(picks.picks.len(), parts.len());
}

type Planned = (AdvancedPlan, Picks, Vec<(String, usize)>);

fn plan_of(feeders_text: &str) -> Planned {
    let parts = from_csv(
        "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nC1,100n,C,30,10,0,top\nX1,odd,Q,5,5,0,top\n",
    )
    .unwrap();
    let feeders: Vec<FeederSpec> = feeders_from_csv(feeders_text).unwrap();
    let picks = assign(&parts, &feeders, Some(Side::Top));
    let cat = Catalog::new();
    let undefined = cat.undefined(&parts);
    let plan = plan_advanced(
        &parts,
        &feeders,
        &picks,
        &cat,
        &MachineConfig::parse("nozzles = 2").unwrap(),
        &Options::default(),
    )
    .unwrap();
    (plan, picks, undefined)
}

#[test]
fn the_report_lists_the_batches_and_what_is_missing() {
    let (plan, picks, undefined) =
        plan_of("id,value,package,x,y,count,warn_below\nF1,10k,R,-10,0,3,5\nF2,100n,C,-10,10,0,\n");
    let text = report(&plan, &picks, &undefined);
    assert!(
        text.starts_with("2 parts in 1 batches, order nearest, 0 tip changes"),
        "{text}"
    );
    assert!(text.contains("batch 1\n"));
    assert!(text.contains("R1 from F1"));
    assert!(text.contains("no feeder for X1"));
    assert!(text.contains("feeder F2 is short of 1 parts"));
    assert!(text.contains("feeder F1 will have 1 parts left"));
    assert!(text.contains("not described in the tables: 10k / R (2 parts)"));
}

#[test]
fn the_list_of_missing_parts_is_a_csv() {
    let (_, picks, undefined) =
        plan_of("id,value,package,x,y,count\nF1,10k,R,-10,0,3\nF2,100n,C,-10,10,0\n");
    let csv = missing_csv(&picks, &undefined);
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[0], "kind,what,detail");
    assert!(lines.contains(&"no_feeder,X1,"));
    assert!(lines.contains(&"short,F2,1"));
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("undefined,") && l.ends_with(",2")
                || l.starts_with("undefined,\""))
    );
}

#[test]
fn the_gcode_only_moves_the_head_and_picks_every_part_of_a_batch_before_placing_them() {
    let (plan, _, _) =
        plan_of("id,value,package,x,y,count\nF1,10k,R,-10,0,3\nF2,100n,C,-10,10,3\n");
    let text = gcode(&plan);
    assert!(text.starts_with("; simulation of the movements only"));
    assert!(text.contains("G21\nG90\n"));
    let moves = text.lines().filter(|l| l.starts_with("G0 ")).count();
    assert_eq!(moves, 2 * plan.plan.placements.len());
    // inside each batch all the picks come before the first place
    let batches: Vec<&str> = text.split("; batch ").skip(1).collect();
    assert_eq!(batches.len(), plan.plan.batches);
    for b in &batches {
        let picks = b.lines().filter(|l| l.starts_with("; pick")).count();
        let places = b.lines().filter(|l| l.starts_with("; place")).count();
        assert!(picks > 0 && picks == places);
        let last_pick = b
            .lines()
            .enumerate()
            .filter(|(_, l)| l.starts_with("; pick"))
            .map(|(i, _)| i)
            .max()
            .unwrap();
        let first_place = b.lines().position(|l| l.starts_with("; place")).unwrap();
        assert!(
            last_pick < first_place,
            "a place before the last pick in {b}"
        );
    }
    assert!(text.ends_with("; end\n"));
    // no command for a pump or a valve
    assert!(!text.contains("M3") && !text.contains("M8") && !text.contains("M106"));
}

#[test]
fn a_feeder_can_be_built_by_hand_for_the_sheet_tests() {
    let f = FeederSpec::from_basic(Feeder {
        id: "F".into(),
        value: "v".into(),
        package: "p".into(),
        x: 1.0,
        y: 2.0,
        count: 3,
    });
    assert_eq!(f.capacity(), 3);
}
