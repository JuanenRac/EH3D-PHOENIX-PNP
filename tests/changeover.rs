//! What has to be changed on the feeders between one job of a queue and the next: the reels to change, to put in and the ones that may stay, the steps of a queue, the text, the table and
//! `queue-changeover`.

use eh3d_phoenix_pnp::changeover::{between, csv, steps, text, Action};
use eh3d_phoenix_pnp::feeding::{from_csv as feeders_from_csv, FeederSpec};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn feeders(rows: &str) -> Vec<FeederSpec> {
    feeders_from_csv(&format!("id,value,package,x,y,count,enabled,slot\n{rows}")).unwrap()
}

const DAY: &str = "F1,10k,R_0603,-20,10,100,yes,S1\nF2,100n,C_0603,-20,20,100,yes,S2\nF3,MCU,QFN-32,-20,40,50,yes,S3\n";
const EVENING: &str = "F1,10k,R_0603,-20,10,100,yes,S1\nF2,1u,C_0603,-20,20,100,yes,S2\nF3,MCU,QFN-32,-20,40,50,no,S3\nF4,LED,LED_0603,-20,50,50,yes,S4\n";

#[test]
fn the_feeders_that_hold_the_same_kind_stay_and_the_others_are_told_by_what_is_to_be_done() {
    let (changes, kept) = between(&feeders(DAY), &feeders(EVENING));
    assert_eq!(kept, 1);
    let got: Vec<(&str, &str, Action, &str, &str)> = changes
        .iter()
        .map(|c| {
            (
                c.id.as_str(),
                c.slot.as_str(),
                c.action,
                c.from.as_str(),
                c.to.as_str(),
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            ("F2", "S2", Action::Change, "100n / C_0603", "1u / C_0603"),
            ("F3", "S3", Action::Free, "MCU / QFN-32", ""),
            ("F4", "S4", Action::Load, "", "LED / LED_0603"),
        ]
    );
}

#[test]
fn the_kind_is_the_value_and_the_package_without_the_case_or_the_blanks_and_the_same_table_changes_nothing(
) {
    let a = feeders("F1,10k,R_0603,-20,10,100,yes,\n");
    let b = feeders("F1, 10K ,r_0603,-20,10,3,yes,\n");
    let (changes, kept) = between(&a, &b);
    assert!(changes.is_empty());
    assert_eq!(kept, 1);
    // the count of the reel is not a reason to change it
    let (changes, kept) = between(&feeders(DAY), &feeders(DAY));
    assert!(changes.is_empty());
    assert_eq!(kept, 3);
    // a feeder that is off in both is not there for either
    let off = feeders("F9,x,p,0,0,1,no,\n");
    assert_eq!(between(&off, &off), (Vec::new(), 0));
    assert_eq!(between(&[], &[]), (Vec::new(), 0));
}

#[test]
fn a_feeder_that_is_switched_on_after_it_was_off_is_a_reel_to_put_in() {
    let before = feeders("F1,10k,R_0603,-20,10,100,no,\n");
    let after = feeders("F1,10k,R_0603,-20,10,100,yes,\n");
    let (changes, kept) = between(&before, &after);
    assert_eq!(kept, 0);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].action, Action::Load);
    assert_eq!(changes[0].slot, "");
    assert_eq!(Action::Load.name(), "load");
    assert_eq!(Action::Change.name(), "change");
    assert_eq!(Action::Free.name(), "free");
}

fn jobs() -> Vec<(String, Vec<FeederSpec>)> {
    vec![
        ("morning".to_string(), feeders(DAY)),
        ("noon".to_string(), feeders(DAY)),
        ("evening".to_string(), feeders(EVENING)),
    ]
}

#[test]
fn there_is_a_step_between_each_two_jobs_in_the_order_of_the_queue() {
    let found = steps(&jobs());
    assert_eq!(found.len(), 2);
    assert_eq!(
        (found[0].from_job.as_str(), found[0].to_job.as_str()),
        ("morning", "noon")
    );
    assert!(found[0].changes.is_empty());
    assert_eq!((found[0].kept, found[0].work()), (3, 0));
    assert_eq!(
        (found[1].from_job.as_str(), found[1].to_job.as_str()),
        ("noon", "evening")
    );
    // two reels to change or put in, one that may stay: the free one is not work
    assert_eq!(
        (found[1].kept, found[1].changes.len(), found[1].work()),
        (1, 3, 2)
    );
    assert!(steps(&[]).is_empty());
    assert!(steps(&jobs()[..1]).is_empty());
}

#[test]
fn the_text_says_each_step_and_each_feeder_and_how_many_reels_in_all() {
    let t = text(&steps(&jobs()));
    let lines: Vec<&str> = t.lines().collect();
    assert_eq!(
        lines[0],
        "morning -> noon: nothing to change, 3 feeders stay as they are"
    );
    assert_eq!(
        lines[1],
        "noon -> evening: 2 feeders to change or load, 1 stay as they are"
    );
    assert_eq!(
        lines[2],
        "  F2 (slot S2): change 100n / C_0603 for 1u / C_0603"
    );
    assert_eq!(
        lines[3],
        "  F3 (slot S3): MCU / QFN-32 is not used and may stay"
    );
    assert_eq!(lines[4], "  F4 (slot S4): load LED / LED_0603");
    assert_eq!(
        lines[5],
        "2 reels to change or load in 1 of the 2 changes between jobs"
    );
    assert_eq!(lines.len(), 6);
    // jobs that follow each other with nothing to change
    let same = vec![
        ("a".to_string(), feeders(DAY)),
        ("b".to_string(), feeders(DAY)),
        ("c".to_string(), feeders(DAY)),
    ];
    assert!(
        text(&steps(&same)).ends_with("ok: the 3 jobs follow each other with no reel to change\n")
    );
    // fewer than two jobs
    assert_eq!(
        text(&[]),
        "nothing to compare: fewer than two jobs are still to be made\n"
    );
    // a feeder with no slot is told with no place
    let (c, _) = between(&[], &feeders("F1,x,p,0,0,1,yes,\n"));
    let one = vec![eh3d_phoenix_pnp::changeover::Step {
        from_job: "a".to_string(),
        to_job: "b".to_string(),
        changes: c,
        kept: 0,
    }];
    assert!(
        text(&one).contains("\n  F1: load x / p\n"),
        "{}",
        text(&one)
    );
}

#[test]
fn the_table_has_a_row_for_each_feeder_of_each_step() {
    let t = csv(&steps(&jobs()));
    let lines: Vec<&str> = t.lines().collect();
    assert_eq!(lines[0], "from_job,to_job,feeder,slot,action,from,to");
    assert_eq!(
        lines[1],
        "noon,evening,F2,S2,change,100n / C_0603,1u / C_0603"
    );
    assert_eq!(lines[2], "noon,evening,F3,S3,free,MCU / QFN-32,");
    assert_eq!(lines[3], "noon,evening,F4,S4,load,,LED / LED_0603");
    assert_eq!(lines.len(), 4);
    assert_eq!(csv(&[]), "from_job,to_job,feeder,slot,action,from,to\n");
    // a name with a comma is quoted
    let odd = vec![
        ("a,b".to_string(), feeders("F1,x,p,0,0,1,yes,\n")),
        ("c".to_string(), Vec::new()),
    ];
    assert!(csv(&steps(&odd)).contains("\"a,b\",c,F1,,free,x / p,\n"));
}

// ---- queue-changeover

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

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn the_command_compares_the_tables_of_the_jobs_that_are_still_to_be_made() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_changeover_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (day, evening) = (dir.join("day.csv"), dir.join("evening.csv"));
    std::fs::write(
        &day,
        format!("id,value,package,x,y,count,enabled,slot\n{DAY}"),
    )
    .unwrap();
    std::fs::write(
        &evening,
        format!("id,value,package,x,y,count,enabled,slot\n{EVENING}"),
    )
    .unwrap();
    let q = dir.join("q.csv");
    let (pq, pd, pe) = (p(&q), p(&day), p(&evening));
    for (name, table) in [
        ("morning", pd),
        ("noon", pd),
        ("evening", pe),
        ("night", pd),
    ] {
        let o = run(&[
            "queue-add",
            pq,
            name,
            "examples/board.pos",
            table,
            "--boards",
            "5",
        ]);
        assert!(o.status.success(), "{}", err(&o));
    }
    // the job that is left out is not between the others
    assert!(run(&["queue-edit", pq, "night", "skip"]).status.success());
    let o = run(&["queue-changeover", pq]);
    let t = out(&o);
    assert!(!o.status.success(), "{t}");
    assert!(
        t.contains("morning -> noon: nothing to change, 3 feeders stay as they are"),
        "{t}"
    );
    assert!(
        t.contains("noon -> evening: 2 feeders to change or load, 1 stay as they are"),
        "{t}"
    );
    assert!(
        t.contains("  F2 (slot S2): change 100n / C_0603 for 1u / C_0603"),
        "{t}"
    );
    assert!(!t.contains("night"), "{t}");
    assert!(
        err(&o).contains("2 reels have to be changed or put in between the jobs"),
        "{}",
        err(&o)
    );
    // the table, never over a file that is there nor over an input
    let file = dir.join("changes.csv");
    let o = run(&["queue-changeover", pq, "--out", p(&file)]);
    assert!(!o.status.success());
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.starts_with("from_job,to_job,feeder,slot,action,from,to\nnoon,evening,F2,S2,change,"),
        "{text}"
    );
    assert!(!run(&["queue-changeover", pq, "--out", p(&file)])
        .status
        .success());
    assert!(!run(&["queue-changeover", pq, "--out", pd]).status.success());
    assert!(!run(&["queue-changeover", pq, "--out", pq]).status.success());
    // the jobs that are done are not counted: with only the first two left the queue needs no change
    assert!(run(&["queue-edit", pq, "evening", "skip"]).status.success());
    let o = run(&["queue-changeover", pq]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    assert!(
        out(&o).ends_with("ok: the 2 jobs follow each other with no reel to change\n"),
        "{}",
        out(&o)
    );
    // one job is nothing to compare
    assert!(run(&["queue-edit", pq, "noon", "skip"]).status.success());
    let o = run(&["queue-changeover", pq]);
    assert!(o.status.success());
    assert_eq!(
        out(&o),
        "nothing to compare: fewer than two jobs are still to be made\n"
    );
    // bad words and a queue that is not there
    assert!(!run(&["queue-changeover"]).status.success());
    assert!(!run(&["queue-changeover", pq, "--other"]).status.success());
    assert!(!run(&["queue-changeover", p(&dir.join("none.csv"))])
        .status
        .success());
    assert!(out(&run(&["help"])).contains("phoenix-pnp queue-changeover QUEUE [--out F]"));
    let _ = std::fs::remove_dir_all(&dir);
}
