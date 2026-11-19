//! One board shared between machines: whole feeders to each, the work balanced, the same answer for the same tables, and `split-job`.

use eh3d_phoenix_pnp::feeding::{assign, from_csv as feeders_from_csv};
use eh3d_phoenix_pnp::parts::{parse, Part};
use eh3d_phoenix_pnp::splitjob::{split, MAX_MACHINES};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_phoenix-pnp");

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_split_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn example(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join(name),
    )
    .unwrap()
}

/// A board of `counts[i]` resistors of the value `i`, each value in a feeder of its own.
fn board(counts: &[usize]) -> (Vec<Part>, String) {
    let mut pos = String::from("ref,val,package,posx,posy,rot,side\n");
    let mut feeders = String::from("id,value,package,x,y,count\n");
    for (i, n) in counts.iter().enumerate() {
        feeders.push_str(&format!("F{i},V{i},P,0,{i},1000\n"));
        for k in 0..*n {
            pos.push_str(&format!("R{i}_{k},V{i},P,{k},{i},0,top\n"));
        }
    }
    (parse(&pos).unwrap(), feeders)
}

fn shared(counts: &[usize], machines: usize) -> eh3d_phoenix_pnp::splitjob::Split {
    let (parts, feeders) = board(counts);
    let feeders = feeders_from_csv(&feeders).unwrap();
    let picks = assign(&parts, &feeders, None);
    split(&parts, &feeders, &picks, machines, 1.0)
}

#[test]
fn every_part_goes_to_one_machine_and_the_parts_of_a_feeder_stay_together() {
    let s = shared(&[9, 7, 5, 4, 3, 2, 1], 3);
    let mut all: Vec<&str> = s.machines.iter().flat_map(|m| m.references()).collect();
    assert_eq!(all.len(), 31);
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), 31, "no part is given twice");
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for m in &s.machines {
        for f in &m.feeders {
            assert!(seen.insert(f.clone()), "feeder {f} is on two machines");
        }
        // every part of the machine is picked from one of its feeders
        assert!(m.parts.iter().all(|(_, f)| m.feeders.contains(f)));
    }
    assert_eq!(seen.len(), 7);
}

#[test]
fn the_big_feeders_are_given_first_to_the_machine_that_has_the_least() {
    let s = shared(&[9, 7, 5, 4, 3, 2, 1], 3);
    let feeders: Vec<&[String]> = s.machines.iter().map(|m| m.feeders.as_slice()).collect();
    // 9 -> m1, 7 -> m2, 5 -> m3, 4 -> m3 (the least, 5), 3 -> m2 (7), 2 -> m1 (9, as m3: the lowest number wins a tie), 1 -> m3 (9)
    assert_eq!(feeders[0], ["F0", "F5"]);
    assert_eq!(feeders[1], ["F1", "F4"]);
    assert_eq!(feeders[2], ["F2", "F3", "F6"]);
    let seconds: Vec<f64> = s.machines.iter().map(|m| m.seconds).collect();
    assert_eq!(seconds, [11.0, 10.0, 10.0]);
    let (most, least) = s.extremes();
    assert_eq!((most, least), (11.0, 10.0));
}

#[test]
fn one_machine_gets_everything_and_more_machines_than_feeders_leave_some_with_nothing() {
    let one = shared(&[3, 2], 1);
    assert_eq!(one.machines.len(), 1);
    assert_eq!(one.machines[0].parts.len(), 5);
    let many = shared(&[3, 2], 4);
    assert_eq!(
        many.machines.iter().filter(|m| m.parts.is_empty()).count(),
        2
    );
    assert!(many.text().contains("machine 3: nothing"));
    assert_eq!(many.extremes(), (3.0, 0.0));
    // 0 machines is taken as 1 and more than the most as the most
    assert_eq!(shared(&[1], 0).machines.len(), 1);
    assert_eq!(shared(&[1], 99).machines.len(), MAX_MACHINES);
}

#[test]
fn the_work_is_never_further_apart_than_the_biggest_feeder() {
    for machines in 2..=5 {
        let counts = [13, 11, 8, 8, 6, 5, 3, 2, 2, 1];
        let s = shared(&counts, machines);
        let (most, least) = s.extremes();
        assert!(most - least <= 13.0, "{machines}: {most} {least}");
        let total: f64 = s.machines.iter().map(|m| m.seconds).sum();
        assert_eq!(total, 59.0);
    }
}

#[test]
fn the_same_tables_give_the_same_split_whatever_the_order_of_the_parts() {
    let (mut parts, feeders) = board(&[5, 4, 4, 3]);
    let feeders = feeders_from_csv(&feeders).unwrap();
    let a = split(&parts, &feeders, &assign(&parts, &feeders, None), 2, 1.0);
    parts.reverse();
    let b = split(&parts, &feeders, &assign(&parts, &feeders, None), 2, 1.0);
    assert_eq!(a, b);
    // a tie between feeders is told by the id: F1 before F2 before F3
    assert_eq!(a.machines[0].feeders, ["F0", "F3"]);
    assert_eq!(a.machines[1].feeders, ["F1", "F2"]);
}

#[test]
fn a_part_that_no_feeder_holds_is_told_and_given_to_nobody() {
    let parts = parse(&example("board.pos")).unwrap();
    let feeders = feeders_from_csv("id,value,package,x,y,count\nF1,10k,R_0603,0,0,100\n").unwrap();
    let s = split(&parts, &feeders, &assign(&parts, &feeders, None), 2, 1.0);
    assert_eq!(s.without_feeder, ["C1", "C2", "D1", "U1"]);
    assert_eq!(s.machines.iter().map(|m| m.parts.len()).sum::<usize>(), 3);
    assert!(s.text().contains("no feeder holds: C1 C2 D1 U1"));
}

#[test]
fn the_csv_has_a_row_for_each_part_with_the_machine_and_the_feeder() {
    let s = shared(&[2, 1], 2);
    assert_eq!(
        s.csv(),
        "machine,feeder,reference\n1,F0,R0_0\n1,F0,R0_1\n2,F1,R1_0\n"
    );
}

#[test]
fn the_command_prints_the_share_of_each_machine_and_writes_the_rows() {
    let o = run(&[
        "split-job",
        "examples/board.pos",
        "examples/feeders.csv",
        "--machines",
        "2",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    assert!(
        text.contains("machine 1: 4 parts from 2 feeders (F1, F4), about 3.2 s"),
        "{text}"
    );
    assert!(
        text.contains("machine 2: 3 parts from 2 feeders (F2, F3), about 2.4 s"),
        "{text}"
    );
    assert!(text.contains("0.8 s apart"), "{text}");
    let dir = temp("out");
    let file = dir.join("split.csv").to_string_lossy().into_owned();
    let o = run(&[
        "split-job",
        "examples/board.pos",
        "examples/feeders.csv",
        "--machines",
        "2",
        "--out",
        &file,
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let csv = std::fs::read_to_string(&file).unwrap();
    assert_eq!(csv.lines().count(), 8);
    assert!(csv.contains("1,F1,R1") && csv.contains("2,F3,U1"), "{csv}");
    let again = run(&[
        "split-job",
        "examples/board.pos",
        "examples/feeders.csv",
        "--machines",
        "2",
        "--out",
        &file,
    ]);
    assert_eq!(again.status.code(), Some(1));
    assert!(err(&again).contains("PHOENIX-PNP-040"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_command_uses_the_times_of_the_settings_and_exits_with_1_for_a_part_with_no_feeder() {
    let dir = temp("cfg");
    let cfg = dir.join("m.cfg");
    std::fs::write(&cfg, "pick_s = 1\nplace_s = 1\n").unwrap();
    let c = cfg.to_string_lossy().into_owned();
    let o = run(&[
        "split-job",
        "examples/board.pos",
        "examples/feeders.csv",
        "--machines",
        "1",
        "--config",
        &c,
    ]);
    assert!(
        out(&o).contains("7 parts from 4 feeders (F1, F2, F3, F4), about 14.0 s"),
        "{}",
        out(&o)
    );
    let feeders = dir.join("feeders.csv");
    std::fs::write(
        &feeders,
        "id,value,package,x,y,count\nF1,10k,R_0603,0,0,100\n",
    )
    .unwrap();
    let o = run(&[
        "split-job",
        "examples/board.pos",
        &feeders.to_string_lossy(),
        "--machines",
        "2",
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("4 parts have no feeder"), "{}", err(&o));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_command_checks_the_number_of_machines_and_its_arguments() {
    for args in [
        vec!["split-job", "examples/board.pos", "examples/feeders.csv"],
        vec![
            "split-job",
            "examples/board.pos",
            "examples/feeders.csv",
            "--machines",
            "0",
        ],
        vec![
            "split-job",
            "examples/board.pos",
            "examples/feeders.csv",
            "--machines",
            "17",
        ],
        vec![
            "split-job",
            "examples/board.pos",
            "examples/feeders.csv",
            "--machines",
            "2.5",
        ],
        vec!["split-job", "examples/board.pos", "--machines", "2"],
        vec![
            "split-job",
            "examples/board.pos",
            "examples/feeders.csv",
            "--machines",
            "2",
            "extra",
        ],
    ] {
        let o = run(&args);
        assert_eq!(o.status.code(), Some(1), "{args:?}");
        assert!(err(&o).starts_with("error:"), "{args:?}");
    }
}
