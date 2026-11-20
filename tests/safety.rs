//! The rules that only make a machine safer after something goes wrong. These tests walk the events and fail if any change to the code lets an event lower a level, raise the speed, or let a
//! machine that was held or stopped move again by itself: every sequence of events up to a length is tried, and many longer ones made by chance on a fixed seed.

use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::gcodedriver::DriverProfile;
use eh3d_phoenix_pnp::safety::{replay, replay_text, Event, Governor, Level, Rules};

fn rules() -> Rules {
    Rules::from_profile(&DriverProfile::default())
}

/// The state that must never relax: the level, the speed and whether moves are allowed.
fn restraint(g: &Governor) -> (Level, f64, bool) {
    let l = g.limits();
    (g.level(), l.speed_pct, l.moves)
}

fn relaxed(before: (Level, f64, bool), after: (Level, f64, bool)) -> bool {
    after.0 < before.0 || after.1 > before.1 || (after.2 && !before.2)
}

#[test]
fn no_event_ever_lowers_a_level_raises_the_speed_or_lets_a_held_machine_move() {
    // every sequence of up to five events, 7 + 49 + ... + 16807 of them
    let mut stack: Vec<Vec<Event>> = vec![vec![]];
    let mut checked = 0usize;
    while let Some(seq) = stack.pop() {
        let mut g = Governor::new(rules());
        let mut before = restraint(&g);
        for e in &seq {
            g.observe(*e);
            let after = restraint(&g);
            assert!(
                !relaxed(before, after),
                "{seq:?} relaxed {before:?} to {after:?}"
            );
            before = after;
        }
        checked += 1;
        if seq.len() < 5 {
            for e in Event::ALL {
                let mut next = seq.clone();
                next.push(e);
                stack.push(next);
            }
        }
    }
    assert!(checked > 19_000, "{checked}");
}

#[test]
fn long_runs_made_by_chance_keep_the_rule_too() {
    let mut state = 99u64;
    let mut rnd = move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 33) as usize
    };
    for _ in 0..500 {
        let mut g = Governor::new(rules());
        let mut before = restraint(&g);
        for _ in 0..60 {
            let e = Event::ALL[rnd() % Event::ALL.len()];
            g.observe(e);
            let after = restraint(&g);
            assert!(!relaxed(before, after));
            before = after;
        }
    }
}

#[test]
fn good_news_gives_nothing_back() {
    let mut g = Governor::new(rules());
    g.observe(Event::Timeout);
    let level = g.level();
    let speed = g.limits().speed_pct;
    for _ in 0..1000 {
        g.observe(Event::Success);
    }
    assert_eq!(g.level(), level);
    assert_eq!(g.limits().speed_pct, speed);
    assert!(level > Level::Normal && speed < 100.0);
}

#[test]
fn a_stopped_machine_stays_stopped_and_cannot_be_released() {
    let mut g = Governor::new(rules());
    assert_eq!(g.observe(Event::ErrorReply), Level::Stopped);
    for _ in 0..10 {
        g.observe(Event::Success);
    }
    assert!(g.acknowledge().is_err());
    assert_eq!(g.level(), Level::Stopped);
    assert!(!g.limits().moves);
}

#[test]
fn an_acknowledgement_brings_a_held_machine_to_slow_and_never_to_normal() {
    let mut g = Governor::new(rules());
    g.observe(Event::PositionMismatch);
    assert_eq!(g.level(), Level::Hold);
    assert_eq!(g.acknowledge().unwrap(), Level::Slow);
    assert!(g.limits().moves);
    assert_eq!(g.limits().speed_pct, rules().slow_speed_pct);
    // a machine that is not held has nothing to acknowledge, and the acknowledgement cannot be used to speed it up
    assert!(g.acknowledge().is_err());
    assert_eq!(g.level(), Level::Slow);
    // one more trouble after the release counts from where it was
    g.observe(Event::Timeout);
    assert!(g.level() >= Level::Slow);
}

#[test]
fn the_events_that_hold_or_stop_do_it_at_once() {
    for (e, level) in [
        (Event::PositionMismatch, Level::Hold),
        (Event::VacuumLost, Level::Hold),
        (Event::ErrorReply, Level::Stopped),
    ] {
        let mut g = Governor::new(rules());
        assert_eq!(g.observe(e), level, "{e:?}");
    }
}

#[test]
fn troubles_add_up_to_careful_then_slow_then_held() {
    let mut g = Governor::new(rules());
    let r = rules();
    let mut levels = Vec::new();
    for _ in 0..r.hold_events {
        levels.push(g.observe(Event::Retry));
    }
    assert_eq!(levels[(r.careful_events - 1) as usize], Level::Careful);
    assert_eq!(levels[(r.slow_events - 1) as usize], Level::Slow);
    assert_eq!(*levels.last().unwrap(), Level::Hold);
    assert_eq!(g.limits().speed_pct, 0.0);
}

#[test]
fn the_speed_of_each_level_is_the_one_of_the_rules_and_falls() {
    let r = rules();
    let mut g = Governor::new(r.clone());
    assert_eq!(g.limits().speed_pct, 100.0);
    g.observe(Event::Timeout);
    assert_eq!(g.limits().speed_pct, r.careful_speed_pct);
    for _ in 0..r.slow_events {
        g.observe(Event::Timeout);
    }
    assert_eq!(g.limits().speed_pct, r.slow_speed_pct);
}

#[test]
fn rules_that_would_let_a_trouble_speed_the_machine_up_are_refused() {
    assert!(rules().check().is_ok());
    let mut r = rules();
    r.slow_speed_pct = r.careful_speed_pct + 1.0;
    assert!(r.check().is_err());
    let mut r = rules();
    r.careful_speed_pct = 101.0;
    assert!(r.check().is_err());
    let mut r = rules();
    r.hold_events = r.slow_events;
    assert!(r.check().is_err());
    let mut r = rules();
    r.careful_events = 0;
    assert!(r.check().is_err());
    let mut r = rules();
    r.slow_speed_pct = 0.0;
    assert!(r.check().is_err());
}

#[test]
fn the_history_keeps_every_event_with_the_level_after_it() {
    let mut g = Governor::new(rules());
    g.observe(Event::Success);
    g.observe(Event::Timeout);
    g.observe(Event::ErrorReply);
    assert_eq!(g.history.len(), 3);
    assert_eq!(g.history[0], (Event::Success, Level::Normal));
    assert_eq!(g.history[2].1, Level::Stopped);
}

#[test]
fn a_file_of_events_is_replayed_and_the_words_are_read_in_any_case() {
    let text = include_str!("../examples/safety_events.txt");
    let r = replay(&rules(), text);
    assert!(r.unknown.is_empty());
    assert_eq!(r.rows.len(), 13);
    // the level in the rows never falls except at the acknowledgement
    let mut last = Level::Normal;
    for (w, level, _, _) in &r.rows {
        if w != "ack" {
            assert!(*level >= last, "{w}");
        }
        last = *level;
    }
    assert_eq!(r.rows.last().unwrap().1, Level::Slow);
    let r = replay(&rules(), "TIMEOUT\n  Retry  # a comment\nnonsense\n\n");
    assert_eq!(r.rows.len(), 2);
    assert_eq!(r.unknown, vec![(3, "nonsense".to_string())]);
}

#[test]
fn the_replay_is_written_in_the_eight_languages() {
    let r = replay(&rules(), "timeout\nbogus\n");
    let mut seen = std::collections::BTreeSet::new();
    for l in Lang::ALL {
        let t = replay_text(&r, l);
        assert!(t.contains("timeout") && t.contains("bogus"));
        assert!(!t.contains("{n}") && !t.contains("{w}"));
        seen.insert(t);
    }
    assert_eq!(seen.len(), 8);
}

#[test]
fn the_figures_of_every_example_profile_keep_the_rule() {
    for text in [
        include_str!("../examples/driver_marlin.cfg"),
        include_str!("../examples/driver_klipper.cfg"),
    ] {
        let p = DriverProfile::parse(text).unwrap();
        Rules::from_profile(&p).check().unwrap();
    }
}
