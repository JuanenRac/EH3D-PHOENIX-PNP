//! The queue of jobs: the file with its checksum, the states of an entry, the gate of the confirmation, the five commands and the tool of the window.

use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::error::{Code, Lang};
use eh3d_phoenix_pnp::gui::toolbox::{queue_text, Tool, Toolbox};
use eh3d_phoenix_pnp::gui::workspace::Workspace;
use eh3d_phoenix_pnp::queue::{
    identity, name_ok, read_file, write_file, Entry, Kind, Queue, State, MAX_BOARDS, MAX_ENTRIES,
};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const MACHINE: &str = include_str!("../examples/machine.cfg");

fn entry(name: &str, boards: u32) -> Entry {
    Entry::new(name, Kind::Parts, "board.pos", "feeders.csv", boards)
}

fn queue_of(names: &[&str]) -> Queue {
    let mut q = Queue::new();
    for n in names {
        q.add(entry(n, 2)).unwrap();
    }
    q
}

// ---- the entries

#[test]
fn an_entry_needs_a_name_files_a_side_and_a_number_of_boards() {
    for ok in ["a", "board-1", "P_2.v3", &"x".repeat(64)] {
        assert!(name_ok(ok), "{ok}");
    }
    for bad in [
        "",
        "a b",
        "a/b",
        "a,b",
        "..",
        ".",
        "é",
        &"x".repeat(65),
        "a\"b",
    ] {
        assert!(!name_ok(bad), "{bad}");
    }
    let mut q = Queue::new();
    let code = |e: Entry| q.clone().add(e).unwrap_err().code;
    assert_eq!(code(entry("bad name", 1)), Code::QueueBad);
    assert_eq!(code(entry("a", 0)), Code::QueueBad);
    assert_eq!(code(entry("a", MAX_BOARDS + 1)), Code::QueueBad);
    assert_eq!(
        code(Entry::new("a", Kind::Parts, "", "f.csv", 1)),
        Code::QueueBad
    );
    assert_eq!(
        code(Entry::new("a", Kind::Parts, "b.pos", " ", 1)),
        Code::QueueBad
    );
    let mut e = entry("a", 1);
    e.side = "left".to_string();
    assert_eq!(code(e), Code::QueueBad);
    let mut e = entry("a", 3);
    e.done = 4;
    assert_eq!(code(e), Code::QueueBad);
    let mut e = entry("a", 1);
    e.detail = "two\nlines".to_string();
    assert_eq!(code(e), Code::QueueBad);
    let mut e = entry("a", 1);
    e.confirmed = "not a hash!".to_string();
    assert_eq!(code(e), Code::QueueBad);
    // a good one, and the same name twice
    q.add(entry("a", MAX_BOARDS)).unwrap();
    assert_eq!(q.add(entry("a", 1)).unwrap_err().code, Code::QueueBad);
    assert_eq!(q.len(), 1);
}

#[test]
fn a_queue_holds_no_more_entries_than_the_limit() {
    let mut q = Queue::new();
    for i in 0..MAX_ENTRIES {
        q.add(entry(&format!("e{i}"), 1)).unwrap();
    }
    let e = q.add(entry("one_more", 1)).unwrap_err();
    assert!(e.detail.contains("more than"), "{}", e.detail);
    assert_eq!(q.len(), MAX_ENTRIES);
}

#[test]
fn the_arguments_of_an_entry_are_those_of_the_commands_that_plan_it() {
    let mut e = entry("a", 1);
    assert_eq!(e.arguments(), ["board.pos", "feeders.csv"]);
    e.kind = Kind::Panel;
    e.source = "panel.json".to_string();
    e.config = "m.cfg".to_string();
    e.packages = "p.csv".to_string();
    e.catalog = "c.csv".to_string();
    e.side = "bottom".to_string();
    assert_eq!(
        e.arguments(),
        [
            "--config",
            "m.cfg",
            "--packages",
            "p.csv",
            "--catalog",
            "c.csv",
            "--side",
            "bottom",
            "--panel",
            "panel.json",
            "feeders.csv"
        ]
    );
}

// ---- the file

#[test]
fn a_queue_is_written_and_read_back_as_the_same_queue() {
    let mut q = queue_of(&["a", "b", "c"]);
    let mut e = Entry::new(
        "with, comma",
        Kind::Panel,
        "C:\\x y\\panel.json",
        "f,1.csv",
        7,
    );
    e.config = "m.cfg".to_string();
    e.side = "top".to_string();
    e.detail = "said \"no\", twice".to_string();
    q.add(e).unwrap_err();
    let mut e = Entry::new("d", Kind::Panel, "C:\\x y\\panel.json", "f,1.csv", 7);
    e.side = "top".to_string();
    e.detail = "said \"no\", twice".to_string();
    q.add(e).unwrap();
    q.confirm("a", "0123456789abcdef").unwrap();
    q.board_done("a").unwrap();
    q.set_state("b", State::Failed, "no feeder\nholds it")
        .unwrap();
    let text = q.to_text();
    assert!(text.starts_with("name,kind,source,feeders,"), "{text}");
    assert!(text.lines().last().unwrap().starts_with("# checksum = "));
    let back = Queue::parse(&text).unwrap();
    assert_eq!(back, q);
    assert_eq!(back.get("b").unwrap().detail, "no feeder holds it");
    // the text is the same whatever the line endings of the checkout
    assert_eq!(Queue::parse(&text.replace('\n', "\r\n")).unwrap(), q);
    // a byte order mark is let pass
    assert_eq!(Queue::parse(&format!("\u{feff}{text}")).unwrap(), q);
}

#[test]
fn a_queue_that_was_cut_or_changed_by_hand_is_refused_and_one_written_by_hand_is_not() {
    let q = queue_of(&["a", "b"]);
    let text = q.to_text();
    // a change of one character, anywhere above the checksum
    let changed = text.replacen(",2,0,pending", ",3,0,pending", 1);
    assert_ne!(changed, text);
    let e = Queue::parse(&changed).unwrap_err();
    assert_eq!(e.code, Code::QueueBad);
    assert!(e.detail.contains("checksum"), "{}", e.detail);
    // a row taken out, the checksum line cut, text after it, two checksums
    let fewer: Vec<&str> = text.lines().collect();
    let without_row = [fewer[0], fewer[2], fewer[3]].join("\n");
    assert!(Queue::parse(&without_row)
        .unwrap_err()
        .detail
        .contains("checksum"));
    assert!(
        Queue::parse(&format!("{text}b,parts,x,y,,,,,1,0,pending,,\n"))
            .unwrap_err()
            .detail
            .contains("after the checksum")
    );
    let twice = format!("{text}{}", text.lines().last().unwrap());
    assert!(Queue::parse(&twice).unwrap_err().detail.contains("twice"));
    // without the checksum line the file is taken as written by hand, and is read all the same
    let by_hand: String = text
        .lines()
        .filter(|l| !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(Queue::parse(&by_hand).unwrap(), q);
    assert!(Queue::parse(&format!("# a note\n{by_hand}\n")).is_ok());
}

#[test]
fn a_file_that_is_not_a_queue_is_refused_with_the_row() {
    for (text, part) in [
        ("", "empty"),
        ("# only a note\n", "empty"),
        ("name,kind\n", "the first row must be exactly"),
        (
            "name,kind,source,feeders,config,packages,catalog,side,boards,done,state,confirmed,detail\na,parts\n",
            "2 fields, not 13",
        ),
        (
            "name,kind,source,feeders,config,packages,catalog,side,boards,done,state,confirmed,detail\na,tape,b.pos,f.csv,,,,,1,0,pending,,\n",
            "the kind 'tape'",
        ),
        (
            "name,kind,source,feeders,config,packages,catalog,side,boards,done,state,confirmed,detail\na,parts,b.pos,f.csv,,,,,many,0,pending,,\n",
            "'many' is not a whole number",
        ),
        (
            "name,kind,source,feeders,config,packages,catalog,side,boards,done,state,confirmed,detail\na,parts,b.pos,f.csv,,,,,1,0,asleep,,\n",
            "the state 'asleep'",
        ),
        (
            "name,kind,source,feeders,config,packages,catalog,side,boards,done,state,confirmed,detail\na,parts,b.pos,f.csv,,,,,1,0,pending,,\na,parts,b.pos,f.csv,,,,,1,0,pending,,\n",
            "used twice",
        ),
        (
            "name,kind,source,feeders,config,packages,catalog,side,boards,done,state,confirmed,detail\na,parts,\"b.pos,f.csv,,,,,1,0,pending,,\n",
            "row",
        ),
    ] {
        let e = Queue::parse(text).unwrap_err();
        assert_eq!(e.code, Code::QueueBad, "{text}");
        assert!(e.detail.contains(part), "{part}: {}", e.detail);
    }
}

#[test]
fn damaged_queues_never_panic() {
    let mut q = queue_of(&["a", "b", "c"]);
    q.confirm("a", "0123456789abcdef").unwrap();
    let text = q.to_text();
    for cut in 0..text.len() {
        if text.is_char_boundary(cut) {
            let _ = Queue::parse(&text[..cut]);
        }
    }
    for junk in [
        "\0\0\0",
        "\"\"\"",
        ",,,,,,,,,,,,",
        "# checksum = ",
        "é\u{feff}é",
    ] {
        let _ = Queue::parse(junk);
    }
}

// ---- what becomes of an entry

#[test]
fn the_boards_are_counted_and_the_entry_is_done_with_the_last_one() {
    let mut q = queue_of(&["a"]);
    q.board_done("a").unwrap();
    let e = q.get("a").unwrap();
    assert_eq!((e.done, e.state), (1, State::Running));
    assert_eq!(e.detail, "1 of 2 boards made");
    q.board_done("a").unwrap();
    let e = q.get("a").unwrap();
    assert_eq!((e.done, e.state), (2, State::Done));
    // more than the boards is not counted
    q.board_done("a").unwrap();
    assert_eq!(q.get("a").unwrap().done, 2);
    assert_eq!(q.board_done("zz").unwrap_err().code, Code::QueueBad);
    // a reset starts again and takes the confirmation away
    q.confirm("a", "ab12").unwrap_err();
    q.reset("a").unwrap();
    let e = q.get("a").unwrap();
    assert_eq!((e.done, e.state), (0, State::Pending));
    assert!(e.confirmed.is_empty() && e.detail.is_empty());
}

#[test]
fn only_the_entries_still_to_be_made_are_to_do_and_they_are_counted() {
    let mut q = queue_of(&["a", "b", "c", "d", "e"]);
    q.set_state("a", State::Done, "").unwrap();
    q.set_state("b", State::Running, "").unwrap();
    q.set_state("d", State::Failed, "no").unwrap();
    q.skip("e").unwrap();
    let todo: Vec<&str> = q.to_do().map(|e| e.name.as_str()).collect();
    assert_eq!(todo, ["b", "c"]);
    assert_eq!(q.counts(), [1, 1, 1, 1, 1]);
    let t = q.text();
    assert!(t.contains("a: done 0/2 boards, not confirmed"), "{t}");
    assert!(t.contains("d: failed") && t.contains("- no"), "{t}");
    assert!(t.ends_with("5 entries: 1 pending, 1 running, 1 done, 1 failed, 1 skipped\n"));
    assert_eq!(Queue::new().text(), "the queue is empty\n");
    for s in State::ALL {
        assert_eq!(State::parse(s.name()), Some(s));
        assert_eq!(State::parse(&s.name().to_uppercase()), Some(s));
    }
    assert_eq!(State::parse("x"), None);
}

#[test]
fn an_entry_is_taken_out_unless_it_is_running() {
    let mut q = queue_of(&["a", "b"]);
    q.set_state("a", State::Running, "").unwrap();
    assert_eq!(q.remove("a").unwrap_err().code, Code::QueueBad);
    q.remove("b").unwrap();
    assert_eq!(q.len(), 1);
    assert_eq!(q.remove("b").unwrap_err().code, Code::QueueBad);
    q.skip("a").unwrap();
    q.remove("a").unwrap();
    assert!(q.is_empty());
}

// ---- the gate

#[test]
fn a_job_runs_only_when_its_plan_was_confirmed_and_the_plan_is_the_same() {
    let mut q = queue_of(&["a"]);
    let id = identity("0123", &["parts".to_string(), "feeders".to_string()]);
    // not confirmed
    let why = q.allowed("a", &id, true).unwrap_err();
    assert!(why.contains("not confirmed"), "{why}");
    // the gate lifted: it may run
    q.allowed("a", &id, false).unwrap();
    // confirmed
    q.confirm("a", &id).unwrap();
    q.allowed("a", &id, true).unwrap();
    // the files changed by one byte: another plan
    let other = identity("0123", &["parts".to_string(), "feeders ".to_string()]);
    assert_ne!(id, other);
    let why = q.allowed("a", &other, true).unwrap_err();
    assert!(why.contains("another plan"), "{why}");
    // the confirmation is taken back
    q.revoke("a").unwrap();
    assert!(q
        .allowed("a", &id, true)
        .unwrap_err()
        .contains("not confirmed"));
    // an entry that is done, failed or skipped does not run again until it is reset
    for s in [State::Done, State::Failed, State::Skipped] {
        q.set_state("a", s, "").unwrap();
        assert!(q.allowed("a", &id, false).unwrap_err().contains("reset it"));
    }
    assert!(q.confirm("a", &id).is_err());
    assert!(q.allowed("zz", &id, false).is_err());
    // what is confirmed has to be the identity of a plan
    q.reset("a").unwrap();
    for bad in ["", "no good", "a,b"] {
        assert_eq!(
            q.confirm("a", bad).unwrap_err().code,
            Code::QueueBad,
            "{bad}"
        );
    }
    assert_eq!(q.confirm("zz", &id).unwrap_err().code, Code::QueueBad);
}

#[test]
fn the_identity_of_a_plan_depends_on_the_hash_the_files_and_their_order() {
    let f = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let a = identity("h", &f(&["x", "y"]));
    assert_eq!(a.len(), 16);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(a, identity("h", &f(&["x", "y"])));
    assert_ne!(a, identity("h2", &f(&["x", "y"])));
    assert_ne!(a, identity("h", &f(&["y", "x"])));
    assert_ne!(a, identity("h", &f(&["x", "y", ""])));
    // two files are not the same as one made by joining them
    assert_ne!(identity("h", &f(&["xy"])), identity("h", &f(&["x", "y"])));
}

// ---- the file on disk

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("phoenix_pnp_queue_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn the_file_is_written_whole_with_a_copy_of_the_one_it_replaces() {
    let dir = scratch("file");
    let path = dir.join("q.csv");
    let (path_s, big) = (p(&path), 1_000_000);
    // there is no file yet: an empty queue
    assert!(read_file(path_s, big).unwrap().is_empty());
    let mut q = queue_of(&["a"]);
    write_file(path_s, &q).unwrap();
    assert!(!dir.join("q.csv.prev").exists() && !dir.join("q.csv.tmp").exists());
    q.add(entry("b", 1)).unwrap();
    write_file(path_s, &q).unwrap();
    assert_eq!(read_file(path_s, big).unwrap(), q);
    // the copy kept is the queue as it was before the last write
    let prev = read_file(p(&dir.join("q.csv.prev")), big).unwrap();
    assert_eq!(prev.len(), 1);
    // a file that was changed by hand is refused, with its path, and is not written over by a read
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("pending", "done   ");
    std::fs::write(&path, &text).unwrap();
    let e = read_file(path_s, big).unwrap_err();
    assert!(
        e.detail.contains("q.csv") && e.detail.contains("checksum"),
        "{}",
        e.detail
    );
    // too large
    let e = read_file(path_s, 10).unwrap_err();
    assert_eq!(e.code, Code::FileTooBig);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the settings

#[test]
fn the_settings_of_the_queue_have_defaults_and_a_line_in_the_example() {
    let d = MachineConfig::default();
    assert!(d.queue_confirm && d.queue_stop_on_failure);
    let e = MachineConfig::parse(MACHINE).unwrap();
    assert!(e.queue_confirm && e.queue_stop_on_failure);
    for key in ["queue_confirm", "queue_stop_on_failure"] {
        assert!(KEYS.contains(&key));
        assert!(MACHINE.contains(&format!("\n{key} = on")));
    }
    let c = MachineConfig::parse("queue_confirm = off\nqueue_stop_on_failure = no").unwrap();
    assert!(!c.queue_confirm && !c.queue_stop_on_failure);
    assert_eq!(
        MachineConfig::parse("queue_confirm = maybe")
            .unwrap_err()
            .code,
        Code::ConfigBadValue
    );
}

#[test]
fn the_code_has_its_meaning_in_the_eight_languages() {
    assert_eq!(Code::QueueBad.number(), 60);
    assert_eq!(Code::QueueBad.label(), "PHOENIX-PNP-060");
    let mut seen = Vec::new();
    for lang in Lang::ALL {
        let m = Code::QueueBad.message(lang);
        assert!(!m.is_empty() && !seen.contains(&m), "{m}");
        seen.push(m);
    }
}

// ---- the commands

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .current_dir(root())
        .output()
        .unwrap()
}

fn text(o: &[u8]) -> String {
    String::from_utf8_lossy(o).into_owned()
}

fn ok(args: &[&str]) -> String {
    let o = run(args);
    assert!(o.status.success(), "{args:?}: {}", text(&o.stderr));
    text(&o.stdout)
}

fn fails(args: &[&str]) -> String {
    let o = run(args);
    assert!(
        !o.status.success(),
        "{args:?} succeeded: {}",
        text(&o.stdout)
    );
    format!("{}{}", text(&o.stdout), text(&o.stderr))
}

#[test]
fn a_queue_is_made_checked_confirmed_and_run_from_the_command_line() {
    let dir = scratch("cli");
    let q = dir.join("q.csv");
    let q = p(&q);
    ok(&[
        "queue-add",
        q,
        "first",
        "examples/board.pos",
        "examples/feeders.csv",
        "--boards",
        "2",
        "--packages",
        "examples/packages.csv",
    ]);
    ok(&[
        "queue-add",
        q,
        "second",
        "examples/board.pos",
        "examples/feeders.csv",
        "--side",
        "top",
    ]);
    // the paths are kept as absolute ones
    let file = std::fs::read_to_string(q).unwrap();
    assert!(!file.contains(",examples/"), "{file}");
    // nothing is confirmed: both are told apart, and neither runs
    let out = fails(&["queue-status", q, "--check"]);
    assert!(
        out.contains("first: pending 0/2 boards, not confirmed"),
        "{out}"
    );
    assert!(
        out.contains("first: cannot run - 'first' is not confirmed"),
        "{out}"
    );
    let out = fails(&["queue-run", q]);
    assert!(out.contains("first: not run"), "{out}");
    assert!(
        read_file(q, 1_000_000).unwrap().get("first").unwrap().done == 0,
        "a job that was not confirmed was run"
    );
    // confirmed: it runs both boards and is done; the other is still waiting
    let out = ok(&["queue-confirm", q, "first"]);
    assert!(out.starts_with("confirmed 'first' for the plan "), "{out}");
    let out = fails(&["queue-status", q, "--check"]);
    assert!(out.contains("first: can run - plan "), "{out}");
    assert!(out.contains("second: cannot run"), "{out}");
    let out = fails(&["queue-run", q]);
    assert!(out.contains("first: done"), "{out}");
    assert!(out.contains("second: not run"), "{out}");
    let saved = read_file(q, 1_000_000).unwrap();
    let first = saved.get("first").unwrap();
    assert_eq!((first.state, first.done), (State::Done, 2));
    assert_eq!(saved.get("second").unwrap().state, State::Pending);
    assert!(dir.join("q.csv.prev").exists());
    // a done job is not run again, and the other can be run alone after it is confirmed
    ok(&["queue-confirm", q, "second"]);
    let out = ok(&["queue-run", q, "--only", "second"]);
    assert!(out.contains("second: done"), "{out}");
    assert_eq!(ok(&["queue-run", q]), "nothing to run\n");
    // a reset puts it back, without the confirmation
    ok(&["queue-edit", q, "first", "reset"]);
    let first = read_file(q, 1_000_000).unwrap();
    let first = first.get("first").unwrap();
    assert_eq!((first.state, first.done), (State::Pending, 0));
    assert!(first.confirmed.is_empty());
    assert!(fails(&["queue-run", q, "--only", "first"]).contains("not confirmed"));
    ok(&["queue-edit", q, "first", "skip"]);
    ok(&["queue-edit", q, "first", "remove"]);
    assert_eq!(read_file(q, 1_000_000).unwrap().len(), 1);
    assert!(!dir.join("q.csv.lock").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_gate_can_be_lifted_in_the_settings_and_a_changed_file_voids_the_confirmation() {
    let dir = scratch("gate");
    let q = dir.join("q.csv");
    let q = p(&q);
    let feeders = dir.join("feeders.csv");
    std::fs::copy(root().join("examples/feeders.csv"), &feeders).unwrap();
    ok(&["queue-add", q, "a", "examples/board.pos", p(&feeders)]);
    ok(&["queue-confirm", q, "a"]);
    // the feeders are changed by a byte: the plan the person saw is not the one that would run
    let plain = std::fs::read_to_string(&feeders).unwrap();
    // the line endings are the other ones: the text is the same to the program and not to the identity
    let lf = plain.replace("\r\n", "\n");
    let changed = if lf == plain {
        lf.replace('\n', "\r\n")
    } else {
        lf
    };
    assert_ne!(changed, plain);
    std::fs::write(&feeders, changed).unwrap();
    let out = fails(&["queue-run", q]);
    assert!(out.contains("confirmed for another plan"), "{out}");
    assert_eq!(read_file(q, 1_000_000).unwrap().get("a").unwrap().done, 0);
    // with the gate lifted (for the simulated machine) the job runs
    let cfg = dir.join("queue.cfg");
    std::fs::write(&cfg, "queue_confirm = off\n").unwrap();
    let out = ok(&["queue-run", q, "--config", p(&cfg)]);
    assert!(out.contains("a: done"), "{out}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_job_that_fails_stops_the_queue_or_not_as_the_settings_say() {
    let dir = scratch("fail");
    let q = dir.join("q.csv");
    let q = p(&q);
    let broken = dir.join("broken.pos");
    std::fs::copy(root().join("examples/board.pos"), &broken).unwrap();
    ok(&["queue-add", q, "bad", p(&broken), "examples/feeders.csv"]);
    ok(&[
        "queue-add",
        q,
        "good",
        "examples/board.pos",
        "examples/feeders.csv",
    ]);
    // the table of parts of 'bad' is gone when its turn comes
    std::fs::remove_file(&broken).unwrap();
    let cfg = dir.join("q.cfg");
    std::fs::write(&cfg, "queue_confirm = off\n").unwrap();
    let out = fails(&["queue-run", q, "--config", p(&cfg)]);
    assert!(out.contains("bad: failed"), "{out}");
    let saved = read_file(q, 1_000_000).unwrap();
    assert_eq!(saved.get("bad").unwrap().state, State::Failed);
    assert!(!saved.get("bad").unwrap().detail.is_empty());
    // the queue stopped there
    assert_eq!(saved.get("good").unwrap().state, State::Pending);
    // with the other setting it goes on to the next job
    std::fs::write(&cfg, "queue_confirm = off\nqueue_stop_on_failure = off\n").unwrap();
    ok(&["queue-edit", q, "bad", "reset"]);
    let out = fails(&["queue-run", q, "--config", p(&cfg)]);
    assert!(out.contains("good: done"), "{out}");
    let saved = read_file(q, 1_000_000).unwrap();
    assert_eq!(saved.get("good").unwrap().state, State::Done);
    assert_eq!(saved.get("bad").unwrap().state, State::Failed);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_commands_refuse_what_they_cannot_do() {
    let dir = scratch("refuse");
    let q = dir.join("q.csv");
    let q = p(&q);
    // files that are not there, options that do not exist, a name that is not one
    assert!(
        fails(&["queue-add", q, "a", "no_such.pos", "examples/feeders.csv"])
            .contains("no such file")
    );
    assert!(fails(&[
        "queue-add",
        q,
        "a",
        "examples/board.pos",
        "examples/feeders.csv",
        "--oops"
    ])
    .contains("unknown option"));
    assert!(fails(&[
        "queue-add",
        q,
        "a",
        "examples/board.pos",
        "examples/feeders.csv",
        "--boards",
        "0"
    ])
    .contains("--boards"));
    assert!(fails(&[
        "queue-add",
        q,
        "bad name",
        "examples/board.pos",
        "examples/feeders.csv"
    ])
    .contains("PHOENIX-PNP-060"));
    assert!(fails(&[
        "queue-add",
        q,
        "a",
        "examples/board.pos",
        "examples/feeders.csv",
        "--side",
        "left"
    ])
    .contains("PHOENIX-PNP-060"));
    assert!(
        !Path::new(q).exists(),
        "a queue was made by a command that failed"
    );
    ok(&[
        "queue-add",
        q,
        "a",
        "examples/board.pos",
        "examples/feeders.csv",
    ]);
    assert!(fails(&[
        "queue-add",
        q,
        "a",
        "examples/board.pos",
        "examples/feeders.csv"
    ])
    .contains("used twice"));
    assert!(fails(&["queue-confirm", q, "zz"]).contains("no entry"));
    assert!(fails(&["queue-edit", q, "a", "explode"]).contains("not skip, reset"));
    assert!(fails(&["queue-edit", q, "zz", "skip"]).contains("PHOENIX-PNP-060"));
    assert!(fails(&["queue-edit", q]).contains("needs"));
    assert!(fails(&["queue-run"]).contains("needs"));
    assert!(fails(&["queue-status"]).contains("needs"));
    // a queue that was changed by hand is refused by every command, in the language asked for
    let text = std::fs::read_to_string(q)
        .unwrap()
        .replace(",1,0,pending", ",9,0,pending");
    std::fs::write(q, text).unwrap();
    for cmd in ["queue-status", "queue-run"] {
        let out = fails(&[cmd, q, "--lang", "es"]);
        assert!(
            out.contains("PHOENIX-PNP-060: La cola de trabajos no es válida"),
            "{cmd}: {out}"
        );
    }
    assert!(fails(&["queue-confirm", q, "a"]).contains("checksum"));
    // a lock left by a run that stopped is told, and taken away by the person
    std::fs::write(q, queue_of(&["a"]).to_text()).unwrap();
    std::fs::write(dir.join("q.csv.lock"), "").unwrap();
    assert!(fails(&["queue-run", q]).contains("another queue-run"));
    assert!(
        dir.join("q.csv.lock").exists(),
        "a lock that is not ours was taken away"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the tool of the window

#[test]
fn the_tool_of_the_window_reads_a_queue_and_writes_it_under_its_name_only_to_a_new_file() {
    let mut q = queue_of(&["a", "b"]);
    q.skip("b").unwrap();
    let (info, file) = queue_text(&q.to_text(), Lang::En).unwrap();
    assert!(
        info.contains("a: pending 0/2 boards, not confirmed"),
        "{info}"
    );
    assert!(info.ends_with("2 entries: 1 pending, 0 running, 0 done, 0 failed, 1 skipped"));
    assert_eq!(file, q.to_text());
    // a queue that was changed is refused in the language of the window
    let e = queue_text(&q.to_text().replace("pending", "done   "), Lang::Es).unwrap_err();
    assert!(e.contains("PHOENIX-PNP-060"), "{e}");
    // the tool
    let ws = Workspace::new(Lang::En);
    let mut tb = Toolbox::new(&MachineConfig::default());
    tb.tool = Tool::Queue;
    assert!(Tool::Queue.reads_text());
    assert_eq!(Tool::from_key("queue"), Some(Tool::Queue));
    assert_eq!(Tool::Queue.output_name(), None);
    tb.queue_text = q.to_text();
    tb.run(&ws, Lang::En);
    assert!(tb.error.is_empty(), "{}", tb.error);
    assert_eq!(tb.made_by, Some(Tool::Queue));
    assert!(tb.info.contains("2 entries"));
    // there is nothing to write: the window only reads a queue
    assert!(tb
        .save(&std::env::temp_dir().to_string_lossy(), Lang::En)
        .is_none());
    tb.queue_text = "garbage".to_string();
    tb.run(&ws, Lang::En);
    assert!(tb.error.contains("PHOENIX-PNP-060"), "{}", tb.error);
}
