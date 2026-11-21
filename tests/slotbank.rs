//! The physical occupation of the slots of the banks of feeders: how many slots a tape takes, that no two feeders overlap and that each one is inside the bank and inside the reach of the head,
//! the states free, occupied, depleted and jammed with the table of moves between them walked completely, the table of the banks, and the link with the chain of a feeder node.

use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::nodelink::Feeders;
use eh3d_phoenix_pnp::nodesim::SimFeeders;
use eh3d_phoenix_pnp::slotbank::{next, Bank, Event, Geometry, Layout, Seen, State};

fn geo() -> Geometry {
    Geometry::from_config(&MachineConfig::default())
}

const TABLE: &str = include_str!("../examples/slot_bank.csv");

#[test]
fn a_tape_takes_the_slots_its_width_and_the_gap_ask_for() {
    let g = geo();
    for (tape, slots) in [
        (8.0, 3),
        (12.0, 4),
        (16.0, 5),
        (24.0, 7),
        (32.0, 9),
        (2.0, 2),
    ] {
        assert_eq!(g.slots_for(tape).unwrap(), slots, "{tape}");
    }
    // a width in between takes the next whole slot
    assert_eq!(g.slots_for(8.1).unwrap(), 4);
    // the sizes are settings: a finer slot or no gap changes the count
    let fine = Geometry {
        pitch_mm: 2.0,
        gap_mm: 0.0,
        ..g
    };
    assert_eq!(fine.slots_for(8.0).unwrap(), 4);
    for bad in [0.0, 1.0, -8.0, 105.0, f64::NAN, f64::INFINITY] {
        assert!(g.slots_for(bad).is_err(), "{bad}");
    }
}

#[test]
fn a_bank_of_sixty_slots_holds_twenty_tapes_of_eight_millimetres() {
    let mut l = Layout::new(geo());
    for i in 0..20u32 {
        l.place(&format!("F{i}"), Bank::Left, 1 + 3 * i, 8.0, None)
            .unwrap();
    }
    assert_eq!(l.free_slots(Bank::Left), 0);
    assert_eq!(l.free_slots(Bank::Right), 60);
    assert!(l.place("F20", Bank::Left, 1, 8.0, None).is_err());
    assert_eq!(l.first_fit(Bank::Left, 8.0), None);
    // the other side is its own bank
    assert_eq!(l.first_fit(Bank::Right, 8.0), Some(1));
    assert_eq!(l.picture(Bank::Left), "#".repeat(60));
}

#[test]
fn two_feeders_cannot_stand_on_the_same_slot_but_can_stand_side_by_side() {
    let mut l = Layout::new(geo());
    l.place("A", Bank::Left, 1, 8.0, None).unwrap();
    let e = l
        .place("B", Bank::Left, 3, 8.0, None)
        .unwrap_err()
        .to_string();
    assert!(e.contains("taken by A"), "{e}");
    // the slots 1 to 3 are A's; 4 is free, and on the right bank 3 is free too
    l.place("B", Bank::Left, 4, 8.0, None).unwrap();
    l.place("C", Bank::Right, 3, 8.0, None).unwrap();
    // a wide tape that would cover a feeder further along is refused
    assert!(l.place("D", Bank::Left, 6, 12.0, None).is_err());
    assert_eq!(l.state_of(Bank::Left, 2), State::Occupied);
    assert_eq!(l.state_of(Bank::Left, 7), State::Free);
    assert_eq!(l.occupant(Bank::Left, 5).map(|e| e.id.as_str()), Some("B"));
    assert!(l.occupant(Bank::Right, 1).is_none());
}

#[test]
fn a_feeder_outside_the_bank_or_out_of_the_reach_of_the_head_is_refused() {
    let mut l = Layout::new(geo());
    assert!(l.place("A", Bank::Left, 0, 8.0, None).is_err());
    assert!(l.place("A", Bank::Left, 59, 8.0, None).is_err());
    l.place("A", Bank::Left, 58, 4.0, None).unwrap();
    // the head reaches only 100 mm of a bank of slots of 4 mm: 25 slots
    let short = Geometry {
        reach_mm: 100.0,
        ..geo()
    };
    assert_eq!(short.reachable_slots(), 25);
    let mut s = Layout::new(short);
    let e = s
        .place("A", Bank::Right, 24, 8.0, None)
        .unwrap_err()
        .to_string();
    assert!(e.contains("reaches"), "{e}");
    s.place("A", Bank::Right, 23, 8.0, None).unwrap();
    // a free run that lies beyond the reach is not offered
    for i in 0..7u32 {
        s.place(&format!("X{i}"), Bank::Left, 1 + 3 * i, 8.0, None)
            .unwrap();
    }
    assert_eq!(s.first_fit(Bank::Left, 8.0), Some(22));
    assert_eq!(s.first_fit(Bank::Left, 12.0), Some(22));
    assert_eq!(
        s.first_fit(Bank::Left, 16.0),
        None,
        "slots 22 to 25 are the only ones the head reaches and a tape of 16 mm needs 5"
    );
}

#[test]
fn the_table_of_the_moves_between_the_states_is_walked_completely() {
    use Event::*;
    use State::*;
    // every pair of state and event, written out
    let table: [(State, Event, Option<State>); 24] = [
        (Free, Load, Some(Occupied)),
        (Free, Exhaust, None),
        (Free, Jam, None),
        (Free, Clear, None),
        (Free, Reload, None),
        (Free, Remove, None),
        (Occupied, Load, None),
        (Occupied, Exhaust, Some(Depleted)),
        (Occupied, Jam, Some(Jammed)),
        (Occupied, Clear, None),
        (Occupied, Reload, None),
        (Occupied, Remove, Some(Free)),
        (Depleted, Load, None),
        (Depleted, Exhaust, None),
        (Depleted, Jam, None),
        (Depleted, Clear, None),
        (Depleted, Reload, Some(Occupied)),
        (Depleted, Remove, Some(Free)),
        (Jammed, Load, None),
        (Jammed, Exhaust, None),
        (Jammed, Jam, None),
        (Jammed, Clear, Some(Occupied)),
        (Jammed, Reload, None),
        (Jammed, Remove, Some(Free)),
    ];
    for (s, e, want) in table {
        assert_eq!(next(s, e), want, "{} {}", s.name(), e.name());
    }
    // the table covers every state and every event
    for s in State::ALL {
        for e in Event::ALL {
            assert!(table.iter().any(|t| t.0 == s && t.1 == e));
        }
    }
    // from a free slot every state can be reached, and a removal always ends free
    let mut reached = vec![Free];
    let mut i = 0;
    while i < reached.len() {
        for e in Event::ALL {
            if let Some(n) = next(reached[i], e) {
                if !reached.contains(&n) {
                    reached.push(n);
                }
            }
        }
        i += 1;
    }
    assert_eq!(reached.len(), 4);
    for s in [Occupied, Depleted, Jammed] {
        assert_eq!(next(s, Remove), Some(Free));
    }
    // the words of the states read back
    for s in State::ALL {
        assert_eq!(State::parse(s.name()), Some(s));
    }
    assert_eq!(State::parse("broken"), None);
}

#[test]
fn a_feeder_goes_through_its_life_and_a_move_that_cannot_happen_changes_nothing() {
    let mut l = Layout::new(geo());
    l.place("A", Bank::Left, 1, 8.0, Some(1)).unwrap();
    assert_eq!(l.apply("A", Event::Exhaust).unwrap(), State::Depleted);
    assert_eq!(
        l.picture(Bank::Left).chars().take(4).collect::<String>(),
        "ooo."
    );
    // a depleted feeder cannot jam or be cleared
    assert!(l.apply("A", Event::Jam).is_err());
    assert!(l.apply("A", Event::Clear).is_err());
    assert_eq!(l.find("A").unwrap().state, State::Depleted);
    assert_eq!(l.apply("A", Event::Reload).unwrap(), State::Occupied);
    assert_eq!(l.apply("A", Event::Jam).unwrap(), State::Jammed);
    assert_eq!(
        l.picture(Bank::Left).chars().take(4).collect::<String>(),
        "XXX."
    );
    assert!(l.apply("A", Event::Exhaust).is_err());
    assert_eq!(l.apply("A", Event::Clear).unwrap(), State::Occupied);
    // taking it away frees the slots and the chain position for another
    assert_eq!(l.apply("A", Event::Remove).unwrap(), State::Free);
    assert!(l.find("A").is_none());
    l.place("B", Bank::Left, 2, 8.0, Some(1)).unwrap();
    assert!(l.apply("nobody", Event::Jam).is_err());
    assert_eq!(l.counts(), [117, 1, 0, 0]);
}

#[test]
fn the_table_of_the_banks_is_read_checked_and_written_back_the_same() {
    let l = Layout::from_csv(TABLE, geo()).unwrap();
    assert_eq!(l.entries().len(), 6);
    assert_eq!(l.find("F3").unwrap().width, 4);
    assert_eq!(l.find("F5").unwrap().last(), 5);
    assert_eq!(l.free_slots(Bank::Left), 60 - 3 - 3 - 4 - 3);
    let again = Layout::from_csv(&l.to_csv(), geo()).unwrap();
    assert_eq!(l, again);
    assert_eq!(again.to_csv(), l.to_csv());
    let text = l.text();
    assert!(text.contains("left  ###"), "{text}");
    assert!(
        text.contains("6 feeders: 6 occupied, 0 depleted, 0 jammed"),
        "{text}"
    );
    for (bad, fragment) in [
        ("id,bank\nA,left\n", "exactly"),
        ("", "empty"),
        (
            "id,bank,first_slot,tape_mm,state,chain\nA,top,1,8,,\n",
            "left or right",
        ),
        (
            "id,bank,first_slot,tape_mm,state,chain\nA,left,x,8,,\n",
            "not a slot",
        ),
        (
            "id,bank,first_slot,tape_mm,state,chain\nA,left,1,wide,,\n",
            "width of tape",
        ),
        (
            "id,bank,first_slot,tape_mm,state,chain\nA,left,1,8,free,\n",
            "a free slot has no row",
        ),
        (
            "id,bank,first_slot,tape_mm,state,chain\nA,left,1,8,broken,\n",
            "not occupied",
        ),
        (
            "id,bank,first_slot,tape_mm,state,chain\nA,left,1,8,,33\n",
            "from 1 to 32",
        ),
        (
            "id,bank,first_slot,tape_mm,state,chain\nA,left,1,8,,1\nB,left,4,8,,1\n",
            "already A's",
        ),
        (
            "id,bank,first_slot,tape_mm,state,chain\nA,left,1,8,,\nA,right,1,8,,\n",
            "used twice",
        ),
        (
            "id,bank,first_slot,tape_mm,state,chain\nA,left,1,8,,\nB,left,2,8,,\n",
            "row 3",
        ),
        (
            "id,bank,first_slot,tape_mm,state,chain\nA,left,1,8\n",
            "4 fields",
        ),
    ] {
        let e = Layout::from_csv(bad, geo()).unwrap_err().to_string();
        assert!(e.contains(fragment), "{bad:?}: {e}");
    }
    // a state in the table is kept
    let kept = Layout::from_csv(
        "id,bank,first_slot,tape_mm,state,chain\nA,left,1,8,depleted,\nB,left,4,8,jammed,2\n",
        geo(),
    )
    .unwrap();
    assert_eq!(kept.counts(), [114, 0, 1, 1]);
}

#[test]
fn what_a_node_says_moves_the_state_of_the_slot_by_the_table_and_a_silent_node_moves_nothing() {
    let mut l = Layout::new(geo());
    l.place("A", Bank::Left, 1, 8.0, Some(1)).unwrap();
    l.place("B", Bank::Left, 4, 8.0, Some(2)).unwrap();
    l.place("C", Bank::Left, 7, 8.0, None).unwrap();
    let c = l.sync(&[(1, Seen::Fault), (2, Seen::Empty), (3, Seen::Fault)]);
    assert_eq!(c.len(), 2);
    assert_eq!((c[0].id.as_str(), c[0].to), ("A", State::Jammed));
    assert_eq!((c[1].id.as_str(), c[1].to), ("B", State::Depleted));
    // C has no position in the chain: it is not touched
    assert_eq!(l.find("C").unwrap().state, State::Occupied);
    // silence changes nothing
    assert!(l.sync(&[(1, Seen::Unknown), (2, Seen::Unknown)]).is_empty());
    // a fault on a depleted feeder or an empty tape on a jammed one is not a move of the table
    assert!(l.sync(&[(1, Seen::Empty), (2, Seen::Fault)]).is_empty());
    let c = l.sync(&[(1, Seen::Fine), (2, Seen::Fine)]);
    assert_eq!(c.len(), 2);
    assert_eq!((c[0].from, c[0].to), (State::Jammed, State::Occupied));
    assert_eq!((c[1].from, c[1].to), (State::Depleted, State::Occupied));
    assert!(l.sync(&[(1, Seen::Fine), (2, Seen::Fine)]).is_empty());
}

#[test]
fn a_run_of_what_a_node_says_chosen_by_a_fixed_seed_never_breaks_the_table() {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut rnd = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let mut l = Layout::new(geo());
    for i in 0..4u32 {
        l.place(
            &format!("F{i}"),
            Bank::Left,
            1 + 3 * i,
            8.0,
            Some(i as u8 + 1),
        )
        .unwrap();
    }
    let kinds = [Seen::Unknown, Seen::Fine, Seen::Empty, Seen::Fault];
    let mut moves = 0;
    for _ in 0..500 {
        let seen: Vec<(u8, Seen)> = (1..=4).map(|c| (c, kinds[(rnd() % 4) as usize])).collect();
        let before: Vec<State> = l.entries().iter().map(|e| e.state).collect();
        let changes = l.sync(&seen);
        moves += changes.len();
        for c in &changes {
            // every move made is one the table allows, and none leaves the four states or frees a slot
            assert!(
                Event::ALL.iter().any(|e| next(c.from, *e) == Some(c.to)),
                "{c:?}"
            );
            assert_ne!(c.to, State::Free);
        }
        for (i, e) in l.entries().iter().enumerate() {
            if seen[i].1 == Seen::Unknown {
                assert_eq!(e.state, before[i], "a silent node moved a state");
            }
        }
        assert_eq!(l.entries().len(), 4);
    }
    assert!(moves > 100, "{moves}");
}

#[test]
fn the_chain_of_a_feeder_node_drives_the_slots_of_the_banks() {
    let mut l = Layout::from_csv(TABLE, geo()).unwrap();
    let sim = SimFeeders::new(8, Some(100), "t");
    let mut node = Feeders::new(sim.clone(), 3);
    // the six feeders of the table are the positions 1 to 6 of the chain
    let status = node.refresh().unwrap().clone();
    assert_eq!(status.feeders.len(), 8);
    assert!(l.sync(&status.seen()).is_empty());
    sim.jam(2);
    sim.set_remaining(5, Some(0));
    let changes = l.sync(&node.refresh().unwrap().seen());
    let ids: Vec<(&str, State)> = changes.iter().map(|c| (c.id.as_str(), c.to)).collect();
    assert_eq!(ids, [("F2", State::Jammed), ("F5", State::Depleted)]);
    assert_eq!(l.counts()[2..], [1, 1]);
    // the cause goes, a person acknowledges, the slot is occupied again
    assert!(node.ack(2).is_err(), "the cause is still there");
    sim.clear_cause(2);
    node.ack(2).unwrap();
    sim.set_remaining(5, Some(500));
    let changes = l.sync(&node.refresh().unwrap().seen());
    assert_eq!(changes.len(), 2);
    assert_eq!(l.counts()[1], 6);
    // a feeder that stops answering is not taken for a fault
    sim.set_online(3, false);
    assert!(l.sync(&node.refresh().unwrap().seen()).is_empty());
    assert_eq!(l.find("F3").unwrap().state, State::Occupied);
}
