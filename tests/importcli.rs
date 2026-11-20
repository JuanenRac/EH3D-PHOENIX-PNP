//! The commands of the import and of the guided work as a person runs them with the example files: import-openpnp, assist, align-iterate, vacuum-calibrate and test-board-gen.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

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

fn temp(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("phoenix_pnp_import_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn import_openpnp_writes_the_three_tables_and_the_program_plans_with_them() {
    let dir = temp("openpnp");
    let o = run(&[
        "import-openpnp",
        "--machine",
        "examples/openpnp_machine.xml",
        "--packages",
        "examples/openpnp_package.xml",
        "--parts",
        "examples/openpnp_parts.xml",
        "--out-dir",
        p(&dir),
    ]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    let text = out(&o);
    assert!(
        text.contains("4 packages, 4 parts and 4 feeders read; 0 errors, 3 warnings"),
        "{text}"
    );
    assert!(text.contains("warning: machine line"), "{text}");
    for f in ["packages.csv", "catalog.csv", "feeders.csv"] {
        assert!(dir.join(f).is_file(), "{f}");
    }
    let o = run(&[
        "plan",
        "examples/board.pos",
        p(&dir.join("feeders.csv")),
        p(&dir.join("plan.json")),
        "--partial",
    ]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    // the tables are never written over
    let o = run(&[
        "import-openpnp",
        "--packages",
        "examples/openpnp_package.xml",
        "--out-dir",
        p(&dir),
    ]);
    assert_eq!(o.status.code(), Some(1));
    // the files that were read are as they were
    assert_eq!(
        std::fs::read_to_string(root().join("examples/openpnp_parts.xml")).unwrap(),
        include_str!("../examples/openpnp_parts.xml")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn import_openpnp_says_in_the_language_asked_what_it_could_not_read_and_exits_with_one() {
    let dir = temp("bad");
    let bad = dir.join("parts.xml");
    std::fs::write(&bad, "<openpnp-parts>\n<part id=\"a\" package-id=\"P\" height=\"1\" height-units=\"Cubits\"/>\n</openpnp-parts>").unwrap();
    let o = run(&["import-openpnp", "--parts", p(&bad), "--lang", "de"]);
    assert_eq!(o.status.code(), Some(1));
    let text = out(&o);
    assert!(
        text.contains("Zeile 2") && text.contains("Cubits") && text.contains("Fehler"),
        "{text}"
    );
    let o = run(&["import-openpnp"]);
    assert_eq!(o.status.code(), Some(1));
    let o = run(&["import-openpnp", "--parts", "examples/nothing.xml"]);
    assert_eq!(o.status.code(), Some(1));
    // a document with entities is refused, not expanded
    let bomb = dir.join("bomb.xml");
    std::fs::write(&bomb, "<!DOCTYPE x [<!ENTITY a \"aaaa\">]><openpnp-parts/>").unwrap();
    let o = run(&["import-openpnp", "--parts", p(&bomb)]);
    assert_eq!(o.status.code(), Some(1));
    assert!(out(&o).contains("document type"), "{}", out(&o));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn assist_walks_the_board_with_the_answers_of_a_file_and_writes_the_checklist() {
    let dir = temp("assist");
    let list = dir.join("list.csv");
    let o = run(&[
        "assist",
        "examples/board.pos",
        "--feeders",
        "examples/feeders.csv",
        "--answers",
        "examples/assist_answers.txt",
        "--out",
        p(&list),
    ]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    let text = out(&o);
    assert!(text.contains("[0/7] C1: 100n (C_0603), from F2 at X -20 Y 20, to X 10 Y 30, turned 0 degrees, top side"), "{text}");
    assert!(
        text.contains("R3 left for later")
            && text
                .contains("every part has been gone through: 7 placed, 0 left for later, 7 in all"),
        "{text}"
    );
    let csv = std::fs::read_to_string(&list).unwrap();
    assert_eq!(csv.lines().count(), 8);
    assert!(csv.lines().skip(1).all(|l| l.ends_with(",placed")), "{csv}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn assist_with_a_wrong_reference_does_not_move_and_exits_with_one_when_parts_are_left() {
    let dir = temp("assist_wrong");
    let ans = dir.join("a.txt");
    std::fs::write(&ans, "R1\nC1\nback\nquit\n").unwrap();
    let o = run(&[
        "assist",
        "examples/board.pos",
        "--answers",
        p(&ans),
        "--lang",
        "es",
    ]);
    assert_eq!(o.status.code(), Some(1), "{}", out(&o));
    let text = out(&o);
    assert!(
        text.contains("'R1' no es la pieza que toca ahora; la pieza es C1"),
        "{text}"
    );
    assert!(
        text.contains("C1 vuelve a estar por colocar") && text.contains("detenido"),
        "{text}"
    );
    assert!(err(&o).contains("7 parts are not placed"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn assist_reads_the_answers_from_the_keyboard_when_there_is_no_file_and_can_resume() {
    let dir = temp("assist_stdin");
    let list = dir.join("list.csv");
    let mut child = Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args([
            "assist",
            "examples/board.pos",
            "--order",
            "reference",
            "--out",
            p(&list),
        ])
        .current_dir(root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(b"C1\nC2\nquit\n").unwrap();
    }
    let o = child.wait_with_output().unwrap();
    assert_eq!(o.status.code(), Some(1));
    let csv = std::fs::read_to_string(&list).unwrap();
    assert!(csv.contains("C1,placed\nC2,placed\nR1,pending"), "{csv}");
    let ans = dir.join("a.txt");
    std::fs::write(&ans, "R1\nR2\nR3\nU1\nD1\n").unwrap();
    let o = run(&[
        "assist",
        "examples/board.pos",
        "--order",
        "reference",
        "--resume",
        p(&list),
        "--answers",
        p(&ans),
    ]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    assert!(out(&o).contains("2 parts taken from"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn align_iterate_aligns_a_made_up_camera_and_fails_when_it_cannot() {
    let o = run(&["align-iterate", "--iterations", "8"]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    assert!(out(&o).contains("aligned:") && out(&o).contains("evidence=simulator"));
    let o = run(&["align-iterate", "--error", "50,0"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("not_converged"));
    let o = run(&["align-iterate", "--blind", "1"]);
    assert!(err(&o).contains("unreadable"));
    let o = run(&["align-iterate", "--gain", "7"]);
    assert_eq!(o.status.code(), Some(1));
    let o = run(&["align-iterate", "--iterations", "8", "--lang", "ja"]);
    assert!(out(&o).contains("確認"));
}

#[test]
fn align_iterate_takes_its_figures_from_the_settings_and_the_options_win() {
    let dir = temp("align");
    let cfg = dir.join("m.cfg");
    std::fs::write(&cfg, "lml_max_step_mm = 0.05\nlml_max_iterations = 2\n").unwrap();
    let o = run(&["align-iterate", "--config", p(&cfg)]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("not_converged"));
    let o = run(&[
        "align-iterate",
        "--config",
        p(&cfg),
        "--max-step",
        "2",
        "--iterations",
        "8",
    ]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn vacuum_calibrate_prints_the_lines_for_the_settings_and_fails_on_an_overlap() {
    let o = run(&["vacuum-calibrate", "examples/vacuum_readings.csv"]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    assert!(
        out(&o).contains("vacuum_pick_min_kpa = 47.18")
            && out(&o).contains("vacuum_release_max_kpa = 22.73")
    );
    let dir = temp("vacuum");
    let bad = dir.join("r.csv");
    std::fs::write(&bad, "state,kpa\nfree,30\nheld,20\n").unwrap();
    let o = run(&["vacuum-calibrate", p(&bad), "--lang", "it"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(out(&o).contains("si sovrappongono"), "{}", out(&o));
    let report = dir.join("report.txt");
    let o = run(&[
        "vacuum-calibrate",
        "examples/vacuum_readings.csv",
        "--out",
        p(&report),
    ]);
    assert!(o.status.success());
    assert!(std::fs::read_to_string(&report)
        .unwrap()
        .contains("vacuum_pick_min_kpa"));
    let o = run(&[
        "vacuum-calibrate",
        "examples/vacuum_readings.csv",
        "--out",
        p(&report),
    ]);
    assert_eq!(
        o.status.code(),
        Some(1),
        "an output that exists is not written over"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_board_gen_writes_a_board_the_other_commands_read() {
    let dir = temp("board");
    let (parts, feeders) = (dir.join("parts.csv"), dir.join("feeders.csv"));
    let o = run(&[
        "test-board-gen",
        "mixed",
        p(&parts),
        p(&feeders),
        "--seed",
        "3",
        "--count",
        "120",
    ]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    assert!(out(&o).contains("mixed board: 120 parts"));
    let o = run(&["check", p(&parts), p(&feeders)]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    let o = run(&[
        "plan",
        p(&parts),
        p(&feeders),
        p(&dir.join("plan.json")),
        "--nozzles",
        "2",
    ]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    // the same seed, the same bytes
    let (parts2, feeders2) = (dir.join("parts2.csv"), dir.join("feeders2.csv"));
    run(&[
        "test-board-gen",
        "mixed",
        p(&parts2),
        p(&feeders2),
        "--seed",
        "3",
        "--count",
        "120",
    ]);
    assert_eq!(
        std::fs::read(&parts).unwrap(),
        std::fs::read(&parts2).unwrap()
    );
    assert_eq!(
        std::fs::read(&feeders).unwrap(),
        std::fs::read(&feeders2).unwrap()
    );
    let o = run(&["test-board-gen", "mixed", p(&parts), p(&feeders)]);
    assert_eq!(
        o.status.code(),
        Some(1),
        "a file that exists is not written over"
    );
    let o = run(&[
        "test-board-gen",
        "round",
        p(&dir.join("x")),
        p(&dir.join("y")),
    ]);
    assert_eq!(o.status.code(), Some(1));
    let o = run(&["test-board-gen", "grid", "--print", "--count", "3"]);
    assert!(
        o.status.success() && out(&o).contains("R1,10k,R_0402"),
        "{}",
        out(&o)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_new_command_without_its_files_answers_with_an_error_line() {
    for cmd in [
        "import-openpnp",
        "assist",
        "align-iterate",
        "vacuum-calibrate",
        "test-board-gen",
    ] {
        let o = run(&[
            cmd,
            "nothing.txt",
            "nothing2.txt",
            "nothing3.txt",
            "nothing4.txt",
            "--bogus",
        ]);
        assert_eq!(o.status.code(), Some(1), "{cmd}");
        assert!(
            !err(&o).contains("panicked") && !err(&o).is_empty(),
            "{cmd}"
        );
    }
}
