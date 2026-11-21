//! The revision of a shared table: a token of its content, a write that is refused when the file changed since it was read, no half-written file, and the command that uses it.

use eh3d_phoenix_pnp::revision::{is_token, read, replace, token_of, TOKEN_CHARS};
use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_phoenix-pnp");
const KINDS: &str = include_str!("../examples/feeders_kinds.csv");

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_rev_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn the_token_is_short_stable_and_ignores_the_line_ends_but_nothing_else() {
    let a = token_of("id,count\nF1,10\n");
    assert_eq!(a.len(), TOKEN_CHARS);
    assert!(is_token(&a));
    assert_eq!(a, token_of("id,count\nF1,10\n"));
    assert_eq!(a, token_of("id,count\r\nF1,10\r\n"));
    assert_eq!(a, token_of("\u{feff}id,count\nF1,10\n"));
    for other in [
        "id,count\nF1,11\n",
        "id,count\nF1,10",
        "id,count\n F1,10\n",
        "id,count\nf1,10\n",
        "",
    ] {
        assert_ne!(a, token_of(other), "{other:?}");
    }
    for not in ["", "xyz", &a.to_uppercase(), &a[..15], &format!("{a}0")] {
        assert!(!is_token(not), "{not}");
    }
}

#[test]
fn a_write_that_holds_the_revision_replaces_the_file_and_gives_the_new_one() {
    let dir = folder("ok");
    let f = dir.join("table.csv");
    std::fs::write(&f, "id,count\nF1,10\n").unwrap();
    let (text, rev) = read(p(&f), 1000).unwrap();
    assert_eq!(text, "id,count\nF1,10\n");
    let next = replace(p(&f), &rev, "id,count\nF1,99\n", 1000).unwrap();
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "id,count\nF1,99\n");
    assert_eq!(next, token_of("id,count\nF1,99\n"));
    // the new revision holds the next write; the old one no longer does
    assert!(replace(p(&f), &rev, "x", 1000).is_err());
    replace(p(&f), &next, "id,count\nF1,5\n", 1000).unwrap();
    // nothing is left beside the table
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["table.csv"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_write_is_refused_when_the_file_changed_since_it_was_read_and_the_file_is_left_alone() {
    let dir = folder("stale");
    let f = dir.join("table.csv");
    std::fs::write(&f, "id,count\nF1,10\n").unwrap();
    let (_, rev) = read(p(&f), 1000).unwrap();
    // somebody else writes in between
    std::fs::write(&f, "id,count\nF1,10\nF2,20\n").unwrap();
    let e = replace(p(&f), &rev, "id,count\nF1,99\n", 1000)
        .unwrap_err()
        .to_string();
    assert!(e.contains("changed since it was read"), "{e}");
    assert_eq!(
        std::fs::read_to_string(&f).unwrap(),
        "id,count\nF1,10\nF2,20\n"
    );
    // a change of the line ends alone is not a change
    std::fs::write(&f, "id,count\r\nF1,10\r\nF2,20\r\n").unwrap();
    let (_, rev) = read(p(&f), 1000).unwrap();
    std::fs::write(&f, "id,count\nF1,10\nF2,20\n").unwrap();
    replace(p(&f), &rev, "id,count\nF1,1\n", 1000).unwrap();
    // a revision that is not one, a file that is gone and a file that is too big
    assert!(replace(p(&f), "nope", "x", 1000)
        .unwrap_err()
        .to_string()
        .contains("not a revision"));
    let gone = dir.join("gone.csv");
    assert!(replace(p(&gone), &rev, "x", 1000)
        .unwrap_err()
        .to_string()
        .contains("no file to replace"));
    assert!(!gone.exists());
    assert!(read(p(&f), 3).is_err());
    std::fs::write(&f, [0xff, 0xfe, 0x00]).unwrap();
    assert!(read(p(&f), 1000).unwrap_err().to_string().contains("UTF-8"));
    let _ = std::fs::remove_dir_all(&dir);
}

fn run(args: &[&str]) -> (i32, String, String) {
    let o = Command::new(BIN).args(args).output().unwrap();
    (
        o.status.code().unwrap(),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

#[test]
fn the_table_of_feeders_is_refilled_in_place_only_while_it_has_the_revision_that_was_read() {
    let dir = folder("cmd");
    let feeders = dir.join("feeders.csv");
    let refills = dir.join("refills.csv");
    std::fs::write(&feeders, KINDS).unwrap();
    std::fs::write(&refills, "id,count\nF1,250\n").unwrap();
    let (code, rev, _) = run(&["table-revision", p(&feeders)]);
    assert_eq!(code, 0);
    let rev = rev.trim().to_string();
    assert_eq!(rev, token_of(KINDS));
    // without the revision a file that exists is never written over
    let (code, _, err) = run(&[
        "feeder-refill",
        p(&feeders),
        p(&refills),
        "--out",
        p(&feeders),
    ]);
    assert_eq!(code, 1, "{err}");
    assert_eq!(std::fs::read_to_string(&feeders).unwrap(), KINDS);
    // somebody changes the table: the revision no longer holds
    let changed = KINDS.replace("F1,10k", "F1,12k");
    std::fs::write(&feeders, &changed).unwrap();
    let (code, _, err) = run(&[
        "feeder-refill",
        p(&feeders),
        p(&refills),
        "--out",
        p(&feeders),
        "--revision",
        &rev,
    ]);
    assert_eq!(code, 1);
    assert!(err.contains("changed since it was read"), "{err}");
    assert_eq!(std::fs::read_to_string(&feeders).unwrap(), changed);
    // with the revision of what is there now it is replaced, and the answer says the new one
    let (_, now, _) = run(&["table-revision", p(&feeders)]);
    let (code, out, err) = run(&[
        "feeder-refill",
        p(&feeders),
        p(&refills),
        "--out",
        p(&feeders),
        "--revision",
        now.trim(),
    ]);
    assert_eq!(code, 0, "{err}");
    let after = std::fs::read_to_string(&feeders).unwrap();
    assert!(after.contains("F1,12k,R_0603,-20,10,250,"), "{after}");
    assert!(
        out.contains(&format!("(revision {})", token_of(&after))),
        "{out}"
    );
    // the file of the refills is never the one that is written
    let (code, _, _) = run(&[
        "feeder-refill",
        p(&feeders),
        p(&refills),
        "--out",
        p(&refills),
        "--revision",
        &token_of(&std::fs::read_to_string(&refills).unwrap()),
    ]);
    assert_eq!(code, 1);
    // a revision without a place to write, and a file that is not there
    assert_eq!(
        run(&[
            "feeder-refill",
            p(&feeders),
            p(&refills),
            "--revision",
            &rev
        ])
        .0,
        1
    );
    assert_eq!(run(&["table-revision", p(&dir.join("none.csv"))]).0, 1);
    assert_eq!(run(&["table-revision"]).0, 1);
    let _ = std::fs::remove_dir_all(&dir);
}
