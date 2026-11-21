//! The order of the queue can be changed: an entry goes up, down, to the first or to the last place it may take, and what is running or made keeps its place. Moves from a fixed seed never
//! lose or repeat an entry and never move one that is fixed.

use eh3d_phoenix_pnp::queue::{Entry, Kind, Move, Queue, State};
use std::process::Command;

fn queue_of(states: &[(&str, State)]) -> Queue {
    let mut q = Queue::new();
    for (name, state) in states {
        let mut e = Entry::new(name, Kind::Parts, "board.pos", "feeders.csv", 2);
        e.state = *state;
        q.add(e).unwrap();
    }
    q
}

fn names(q: &Queue) -> Vec<String> {
    q.entries().iter().map(|e| e.name.clone()).collect()
}

#[test]
fn an_entry_goes_up_down_to_the_top_and_to_the_bottom() {
    let mut q = queue_of(&[
        ("a", State::Pending),
        ("b", State::Pending),
        ("c", State::Pending),
        ("d", State::Pending),
    ]);
    assert_eq!(q.move_entry("c", Move::Up).unwrap(), 1);
    assert_eq!(names(&q), ["a", "c", "b", "d"]);
    assert_eq!(q.move_entry("a", Move::Down).unwrap(), 1);
    assert_eq!(names(&q), ["c", "a", "b", "d"]);
    assert_eq!(q.move_entry("d", Move::Top).unwrap(), 0);
    assert_eq!(names(&q), ["d", "c", "a", "b"]);
    assert_eq!(q.move_entry("d", Move::Bottom).unwrap(), 3);
    assert_eq!(names(&q), ["c", "a", "b", "d"]);
}

#[test]
fn what_is_running_or_made_keeps_its_place_and_an_entry_stops_below_it() {
    let mut q = queue_of(&[
        ("done", State::Done),
        ("run", State::Running),
        ("x", State::Pending),
        ("y", State::Pending),
        ("z", State::Failed),
    ]);
    // the fixed ones do not move
    assert!(q.move_entry("done", Move::Down).is_err());
    assert!(q.move_entry("run", Move::Bottom).is_err());
    // the top is the place under the last fixed entry, and no entry goes above it
    assert_eq!(q.move_entry("z", Move::Top).unwrap(), 2);
    assert_eq!(names(&q), ["done", "run", "z", "x", "y"]);
    assert!(q.move_entry("z", Move::Up).is_err());
    assert!(q.move_entry("z", Move::Top).is_err());
    assert_eq!(q.move_entry("x", Move::Up).unwrap(), 2);
    assert_eq!(names(&q), ["done", "run", "x", "z", "y"]);
    // the last one cannot go further down, and a name that is not there is told
    assert!(q.move_entry("y", Move::Down).is_err());
    assert!(q.move_entry("y", Move::Bottom).is_err());
    assert!(q.move_entry("nobody", Move::Up).is_err());
    // an entry before a running one cannot go past it, and one after it cannot go up past it
    let mut q = queue_of(&[
        ("a", State::Pending),
        ("run", State::Running),
        ("b", State::Pending),
    ]);
    assert!(q.move_entry("a", Move::Down).is_err());
    assert!(q.move_entry("a", Move::Bottom).is_err());
    assert!(q.move_entry("b", Move::Up).is_err());
    assert!(q.move_entry("b", Move::Top).is_err());
    assert_eq!(names(&q), ["a", "run", "b"]);
}

#[test]
fn moves_from_a_fixed_seed_never_lose_an_entry_or_move_a_fixed_one() {
    let mut seed: u64 = 20_240_607;
    let mut next = |n: u64| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) % n
    };
    let states = [
        ("a", State::Done),
        ("b", State::Pending),
        ("c", State::Running),
        ("d", State::Pending),
        ("e", State::Failed),
        ("f", State::Skipped),
        ("g", State::Pending),
    ];
    let mut q = queue_of(&states);
    for _ in 0..400 {
        let name = states[next(7) as usize].0;
        let to = [Move::Up, Move::Down, Move::Top, Move::Bottom][next(4) as usize];
        let _ = q.move_entry(name, to);
        let mut sorted = names(&q);
        sorted.sort();
        assert_eq!(sorted, ["a", "b", "c", "d", "e", "f", "g"]);
        // the done and the running entries are where they were
        assert_eq!(q.entries()[0].name, "a");
        assert_eq!(q.entries()[2].name, "c");
        // the text of the queue is read back as the same order
        let back = Queue::parse(&q.to_text()).unwrap();
        assert_eq!(names(&back), names(&q));
    }
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn the_command_moves_an_entry_and_tells_where_it_is() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_queuemove_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("jobs.queue");
    let path = file.to_string_lossy().into_owned();
    std::fs::write(
        &file,
        queue_of(&[("one", State::Pending), ("two", State::Pending)]).to_text(),
    )
    .unwrap();
    let o = run(&["queue-edit", &path, "two", "up"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(String::from_utf8_lossy(&o.stdout).contains("number 1"));
    let back = Queue::parse(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(names(&back), ["two", "one"]);
    // it cannot go higher: the command could not do what was asked, so it ends with 1
    assert_eq!(
        run(&["queue-edit", &path, "two", "top"]).status.code(),
        Some(1)
    );
    assert_eq!(
        run(&["queue-edit", &path, "two", "sideways"]).status.code(),
        Some(1)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
