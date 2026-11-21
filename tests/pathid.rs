//! Portable paths: how a path is kept in a saved file and how two paths are told to be the same file, the same on Windows and on Linux (the text is worked on, no file is opened), and the queue
//! that keeps its paths that way.

use eh3d_phoenix_pnp::pathid::{identity_with, portable, same};
use std::process::Command;

#[test]
fn separators_become_a_slash_and_the_windows_prefix_goes() {
    assert_eq!(
        portable("C:\\boards\\one\\parts.pos"),
        "C:/boards/one/parts.pos"
    );
    assert_eq!(
        portable("\\\\?\\C:\\boards\\parts.pos"),
        "C:/boards/parts.pos"
    );
    assert_eq!(portable("//?/C:/boards/parts.pos"), "C:/boards/parts.pos");
    assert_eq!(
        portable("\\\\?\\UNC\\server\\share\\a.csv"),
        "//server/share/a.csv"
    );
    assert_eq!(portable("\\\\server\\share\\a.csv"), "//server/share/a.csv");
    assert_eq!(
        portable("/home/me/boards/parts.pos"),
        "/home/me/boards/parts.pos"
    );
    assert_eq!(portable("boards\\one\\parts.pos"), "boards/one/parts.pos");
}

#[test]
fn dots_doubled_separators_and_a_trailing_one_are_taken_out() {
    assert_eq!(portable("C:\\a\\.\\b\\..\\c\\"), "C:/a/c");
    assert_eq!(portable("/usr//lib/./x/../bin/"), "/usr/lib/bin");
    assert_eq!(portable("a/b/../../c"), "c");
    // nothing above the root of an absolute path, and a relative path keeps the ones it needs
    assert_eq!(portable("/../../etc"), "/etc");
    assert_eq!(portable("C:\\..\\x"), "C:/x");
    assert_eq!(portable("../a/../../b"), "../../b");
    assert_eq!(portable(""), ".");
    assert_eq!(portable("./"), ".");
    assert_eq!(portable("C:"), "C:");
    // a relative path stays relative and an absolute one stays absolute
    assert!(!portable("boards/a.pos").starts_with('/'));
    assert!(portable("/boards/a.pos").starts_with('/'));
}

#[test]
fn it_is_idempotent_and_never_longer_than_the_path_it_came_from() {
    // a fixed list of spellings, worked through twice
    let spellings = [
        "C:\\a\\b\\..\\c",
        "\\\\?\\UNC\\s\\h\\x\\.\\y",
        "/a//b/./c/..",
        "../x/./y",
        "  spaced/path  ",
        "a",
        "/",
        "C:/",
        "ü\\é/ñ",
    ];
    for s in spellings {
        let once = portable(s);
        assert_eq!(portable(&once), once, "{s}");
        assert!(once.len() <= s.trim().len().max(1) + 1, "{s} -> {once}");
        assert!(!once.contains('\\'), "{once}");
    }
}

#[test]
fn two_spellings_of_one_file_have_one_identity_and_capitals_follow_the_system() {
    assert!(same("C:\\a\\b\\..\\c.csv", "C:/a/c.csv"));
    assert!(same("\\\\?\\C:\\a\\c.csv", "C:/a/c.csv"));
    assert!(!same("C:/a/c.csv", "C:/a/d.csv"));
    // folded where capitals do not matter, kept where they do
    assert_eq!(
        identity_with("C:/Boards/Parts.POS", true),
        "C:/BOARDS/PARTS.POS"
    );
    assert_eq!(
        identity_with("C:\\boards\\parts.pos", true),
        identity_with("c:/BOARDS/Parts.pos", true)
    );
    assert_ne!(
        identity_with("/Boards/a", false),
        identity_with("/boards/a", false)
    );
}

#[test]
fn the_queue_keeps_the_paths_with_slashes_and_the_output_is_never_an_input_by_another_spelling() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_pathid_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let queue = dir.join("q.queue");
    let q = queue.to_str().unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
            .args(args)
            .current_dir(&root)
            .output()
            .unwrap()
    };
    let o = run(&[
        "queue-add",
        q,
        "one",
        "examples/board.pos",
        "examples\\feeders.csv",
    ]);
    // a path written with a backslash is a path of the system it is on; on Linux it is not a file, and the command says so
    if cfg!(windows) {
        assert_eq!(
            o.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        let saved = std::fs::read_to_string(&queue).unwrap();
        assert!(!saved.contains('\\'), "{saved}");
        assert!(saved.contains("examples/feeders.csv"), "{saved}");
    } else {
        assert_eq!(o.status.code(), Some(1));
    }
    // the plan is not written over a table of parts named in another way
    let alias = "examples/../examples/board.pos";
    let plan = run(&["plan", "examples/board.pos", "examples/feeders.csv", alias]);
    assert_eq!(plan.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&plan.stderr).contains("also an input"));
    let _ = std::fs::remove_dir_all(&dir);
}
