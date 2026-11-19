//! Whether the feeders hold enough parts for the jobs of a queue, one after the other: the running total, the job in which a reel runs out, the kinds nobody holds, feeders that are off, and `queue-stock`.

use eh3d_phoenix_pnp::feeding::{from_csv as feeders_from_csv, FeederSpec};
use eh3d_phoenix_pnp::parts::{from_csv, Part};
use eh3d_phoenix_pnp::queuestock::{csv, is_enough, needs, text, JobNeed};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BOARD: &str = "ref,val,package,x,y,rot,side\nR1,10k,R_0603,10,10,0,top\nR2,10k,R_0603,20,10,0,top\nR3,10k,R_0603,30,10,0,top\nC1,100n,C_0603,10,20,0,top\nC2,100n,C_0603,20,20,0,top\nU1,MCU,QFN-32,40,15,0,top\n";

fn parts() -> Vec<Part> {
    from_csv(BOARD).unwrap()
}

fn job(name: &str, boards: u64) -> JobNeed {
    JobNeed {
        name: name.to_string(),
        parts: parts(),
        boards,
    }
}

fn feeders(mcu: u32) -> Vec<FeederSpec> {
    feeders_from_csv(&format!(
        "id,value,package,x,y,count\nF1,10k,R_0603,-20,10,100\nF2,100n,C_0603,-20,20,100\nF3,MCU,QFN-32,-20,40,{mcu}\n"
    ))
    .unwrap()
}

#[test]
fn the_reel_runs_out_in_the_job_that_takes_the_total_over_what_it_holds() {
    let jobs = [job("a", 10), job("b", 10), job("c", 10)];
    let kinds = needs(&jobs, &feeders(25));
    let mcu = kinds.iter().find(|k| k.value == "MCU").unwrap();
    assert_eq!((mcu.needed, mcu.available, mcu.short), (30, 25, 5));
    assert_eq!(mcu.runs_out_in.as_deref(), Some("c"));
    assert_eq!(mcu.jobs_served, 2);
    assert_eq!(mcu.feeders, vec!["F3".to_string()]);
    // the others last: 90 resistors in 100, 60 capacitors in 100
    let r = kinds.iter().find(|k| k.value == "10k").unwrap();
    assert_eq!(
        (r.needed, r.short, r.runs_out_in.clone(), r.jobs_served),
        (90, 0, None, 3)
    );
    assert!(!is_enough(&kinds));
    let t = text(&kinds, 3);
    assert!(
        t.starts_with("MCU / QFN-32: 30 needed, 25 in F3, 5 short; it runs out in the job c (2 jobs before it are served whole)\n"),
        "{t}"
    );
    assert!(t.ends_with("short: 1 kinds lack 5 parts for the 3 jobs; the feeders serve the first 2 jobs whole\n"), "{t}");
}

#[test]
fn when_the_feeders_hold_enough_the_answer_is_ok_and_counts_the_kinds() {
    let kinds = needs(&[job("a", 10), job("b", 10)], &feeders(25));
    assert!(is_enough(&kinds));
    assert_eq!(
        text(&kinds, 2),
        "ok: the feeders hold enough for the 2 jobs (3 kinds)\n"
    );
    // exactly what is held is enough: the reel runs out one part later
    let exact = needs(&[job("a", 25)], &feeders(25));
    assert!(is_enough(&exact));
    let over = needs(&[job("a", 26)], &feeders(25));
    assert!(!is_enough(&over));
    assert_eq!(over.iter().find(|k| k.value == "MCU").unwrap().short, 1);
}

#[test]
fn a_kind_that_no_feeder_holds_is_short_by_all_of_it_from_the_first_job_that_needs_it() {
    let table = "ref,val,package,x,y,rot,side\nX1,TL431,SOT-23,1,1,0,top\n";
    let extra = JobNeed {
        name: "b".to_string(),
        parts: from_csv(table).unwrap(),
        boards: 5,
    };
    let kinds = needs(&[job("a", 1), extra], &feeders(25));
    let x = kinds.iter().find(|k| k.value == "TL431").unwrap();
    assert_eq!((x.needed, x.available, x.short), (5, 0, 5));
    assert!(x.feeders.is_empty());
    assert_eq!(x.runs_out_in.as_deref(), Some("b"));
    assert_eq!(x.jobs_served, 1);
    assert!(text(&kinds, 2).contains("TL431 / SOT-23: 5 needed, no feeder holds it, 5 short"));
}

#[test]
fn feeders_of_one_kind_add_up_and_a_feeder_that_is_off_is_not_counted() {
    let table = "id,value,package,x,y,count,enabled\nF1,10K,r_0603,0,0,40,yes\nF2,10k,R_0603,0,0,40,yes\nF3,10k,R_0603,0,0,500,no\nF4,100n,C_0603,0,0,100,yes\nF5,MCU,QFN-32,0,0,30,yes\n";
    let f = feeders_from_csv(table).unwrap();
    let kinds = needs(&[job("a", 30)], &f);
    let r = kinds
        .iter()
        .find(|k| k.value.eq_ignore_ascii_case("10k"))
        .unwrap();
    // 90 resistors are needed, 80 are held in the two feeders that are on
    assert_eq!((r.needed, r.available, r.short), (90, 80, 10));
    assert_eq!(r.feeders, vec!["F1".to_string(), "F2".to_string()]);
}

#[test]
fn a_job_with_no_boards_left_needs_nothing_and_a_huge_number_does_not_overflow() {
    let kinds = needs(&[job("a", 0)], &feeders(1));
    assert!(is_enough(&kinds));
    assert!(kinds.iter().all(|k| k.needed == 0));
    // a kind that is held and not needed is still listed, with what is held
    assert_eq!(kinds.len(), 3);
    let big = needs(&[job("a", u64::MAX), job("b", u64::MAX)], &feeders(1));
    assert!(big.iter().all(|k| k.needed >= 1 && k.short > 0));
    assert_eq!(needs(&[], &feeders(1)).len(), 3);
    assert_eq!(
        text(&needs(&[], &[]), 0),
        "ok: the feeders hold enough for the 0 jobs (0 kinds)\n"
    );
}

#[test]
fn the_kinds_are_sorted_and_the_csv_has_a_line_for_each() {
    let kinds = needs(&[job("a", 10), job("b", 10), job("c", 10)], &feeders(25));
    let order: Vec<&str> = kinds.iter().map(|k| k.package.as_str()).collect();
    assert_eq!(order, vec!["C_0603", "QFN-32", "R_0603"]);
    let text = csv(&kinds);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[0],
        "value,package,needed,available,short,runs_out_in,feeders"
    );
    assert_eq!(lines.len(), 4);
    assert!(lines.contains(&"MCU,QFN-32,30,25,5,c,F3"), "{text}");
    assert!(lines.contains(&"10k,R_0603,90,100,0,,F1"), "{text}");
}

// ---- queue-stock

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

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn the_command_counts_the_jobs_that_are_still_to_be_made_and_names_the_job_of_each_short_reel() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_queuestock_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let table = dir.join("f.csv");
    std::fs::write(
        &table,
        "id,value,package,x,y,count\nF1,10k,R_0603,-20,10,100\nF2,100n,C_0603,-20,20,100\nF3,MCU,QFN-32,-20,40,25\nF4,LED,LED_0603,-20,50,50\n",
    )
    .unwrap();
    let q = dir.join("q.csv");
    let (pq, pt) = (p(&q), p(&table));
    for (name, boards) in [("a", "10"), ("b", "10"), ("c", "10")] {
        let o = run(&[
            "queue-add",
            pq,
            name,
            "examples/board.pos",
            pt,
            "--boards",
            boards,
        ]);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    }
    let o = run(&["queue-stock", pq]);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(!o.status.success(), "{out}");
    assert!(
        out.contains("MCU / QFN-32: 30 needed, 25 in F3, 5 short; it runs out in the job c"),
        "{out}"
    );
    assert!(out.contains("3 jobs"), "{out}");
    assert!(String::from_utf8_lossy(&o.stderr).contains("1 kinds of part fall short for the queue"));
    // the table, with the table of feeders it is for
    let csv_file = dir.join("need.csv");
    let o = run(&["queue-stock", pq, "--out", p(&csv_file)]);
    assert!(!o.status.success());
    let csv_text = std::fs::read_to_string(&csv_file).unwrap();
    assert!(
        csv_text.starts_with("feeders,value,package,needed,available,short,runs_out_in,feeders\n"),
        "{csv_text}"
    );
    assert!(
        csv_text.contains(",MCU,QFN-32,30,25,5,c,F3\n"),
        "{csv_text}"
    );
    // the job that is left out is not counted: the reel then lasts
    assert!(run(&["queue-edit", pq, "c", "skip"]).status.success());
    let o = run(&["queue-stock", pq]);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(
        o.status.success(),
        "{out}{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(
        out.contains("ok: the feeders hold enough for the 2 jobs"),
        "{out}"
    );
    // not over a file, not over an input
    assert!(!run(&["queue-stock", pq, "--out", p(&csv_file)])
        .status
        .success());
    assert!(!run(&["queue-stock", pq, "--out", pt]).status.success());
    // bad words
    assert!(!run(&["queue-stock"]).status.success());
    assert!(!run(&["queue-stock", pq, "--other"]).status.success());
    assert!(!run(&["queue-stock", p(&dir.join("none.csv"))])
        .status
        .success());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_jobs_with_other_tables_of_feeders_are_counted_apart_and_a_queue_with_nothing_to_do_is_said()
{
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_queuestock2_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (ta, tb) = (dir.join("a.csv"), dir.join("b.csv"));
    let rows = "id,value,package,x,y,count\nF1,10k,R_0603,-20,10,100\nF2,100n,C_0603,-20,20,100\n";
    std::fs::write(&ta, format!("{rows}F3,MCU,QFN-32,-20,40,12\n")).unwrap();
    std::fs::write(&tb, format!("{rows}F3,MCU,QFN-32,-20,40,12\n")).unwrap();
    let q = dir.join("q.csv");
    let pq = p(&q);
    // the same 12 parts on two tables: each job takes 10, so neither falls short; on one table the two would
    assert!(run(&[
        "queue-add",
        pq,
        "a",
        "examples/board.pos",
        p(&ta),
        "--boards",
        "10"
    ])
    .status
    .success());
    assert!(run(&[
        "queue-add",
        pq,
        "b",
        "examples/board.pos",
        p(&tb),
        "--boards",
        "10"
    ])
    .status
    .success());
    let o = run(&["queue-stock", pq]);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(
        o.status.success(),
        "{out}{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert_eq!(
        out.matches("ok: the feeders hold enough for the 1 jobs")
            .count(),
        2,
        "{out}"
    );
    assert!(run(&[
        "queue-add",
        pq,
        "c",
        "examples/board.pos",
        p(&ta),
        "--boards",
        "10"
    ])
    .status
    .success());
    let o = run(&["queue-stock", pq]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("it runs out in the job c"));
    // everything skipped: nothing to count
    for n in ["a", "b", "c"] {
        assert!(run(&["queue-edit", pq, n, "skip"]).status.success());
    }
    let o = run(&["queue-stock", pq]);
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("nothing to count"));
    let _ = std::fs::remove_dir_all(&dir);
}
