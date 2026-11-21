// The panels of the commands of the driver and the assistants: what is typed into one is split safely, the command-line program that sits beside the window is run with it, and what it
// prints comes back. The program is the real one (`phoenix-pnp`), so a panel and its command cannot disagree.
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::gui::cmdpanel::{
    program_beside, read_capped, run_blocking, split_args, ArgError, CmdPanel, Command, CommandBox,
    MAX_ARGS, MAX_ARGS_TOTAL, MAX_ARG_BYTES, MAX_OUTPUT_BYTES,
};
use eh3d_phoenix_pnp::gui::texts::tr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn program() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_phoenix-pnp"))
}

fn example(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
        .display()
        .to_string()
}

const LIMIT: Duration = Duration::from_secs(60);

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eh3d_pnp_cmdpanel_{}_{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_fifteen_commands_have_a_name_a_panel_a_title_and_a_hint_each() {
    let mut names: Vec<&str> = Command::ALL.iter().map(|c| c.name()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 15);
    let mut panels: Vec<&str> = Command::ALL.iter().map(|c| c.panel_id()).collect();
    panels.sort_unstable();
    panels.dedup();
    assert_eq!(panels.len(), 15);
    for c in Command::ALL {
        assert_eq!(Command::from_panel(c.panel_id()), Some(c));
        assert!(c.panel_id().starts_with("cmd_"));
        assert!(!c.usage().is_empty());
        for lang in Lang::ALL {
            assert!(!tr(c.title(), lang).is_empty(), "{} {lang:?}", c.name());
            assert!(tr(c.hint(), lang).len() > 20, "{} {lang:?}", c.name());
        }
    }
    assert_eq!(Command::from_panel("parts"), None);
    // the fifteen words are the ones the command line has (a command that is not there would say so)
    for c in Command::ALL {
        let out = run_blocking(&program(), c, Lang::En, &[], LIMIT);
        assert!(out.failed_to_start.is_none());
        assert!(
            !out.text.contains("unknown command"),
            "{}: {}",
            c.name(),
            out.text
        );
    }
}

#[test]
fn what_is_typed_is_split_into_words_and_a_quote_groups_words() {
    let v = |t: &str| split_args(t).unwrap();
    assert_eq!(v(""), Vec::<String>::new());
    assert_eq!(v("   "), Vec::<String>::new());
    assert_eq!(v("a b  c"), ["a", "b", "c"]);
    assert_eq!(
        v("--seed 3 \"my folder\\p.csv\""),
        ["--seed", "3", "my folder\\p.csv"]
    );
    assert_eq!(v("a\"b c\"d"), ["ab cd"]);
    assert_eq!(v("a \"\" b"), ["a", "", "b"]);
    // a backslash is a backslash and nothing else is special
    assert_eq!(
        v("C:\\a\\b.cfg $HOME ; rm -rf * | x &"),
        [
            "C:\\a\\b.cfg",
            "$HOME",
            ";",
            "rm",
            "-rf",
            "*",
            "|",
            "x",
            "&"
        ]
    );
}

#[test]
fn the_arguments_that_are_refused_are_told_why() {
    assert_eq!(split_args("a \"b"), Err(ArgError::OpenQuote));
    assert_eq!(split_args("a\nb"), Err(ArgError::Control));
    assert_eq!(split_args("a\u{1b}[2J"), Err(ArgError::Control));
    assert_eq!(split_args("a \"b\tc\""), Err(ArgError::Control));
    let many = "x ".repeat(MAX_ARGS + 1);
    assert_eq!(split_args(&many), Err(ArgError::TooMany));
    assert_eq!(split_args(&"x ".repeat(MAX_ARGS)).unwrap().len(), MAX_ARGS);
    assert_eq!(
        split_args(&"y".repeat(MAX_ARG_BYTES + 1)),
        Err(ArgError::TooLong)
    );
    assert_eq!(split_args(&"y".repeat(MAX_ARG_BYTES)).unwrap().len(), 1);
    // each one is within the limit but the sum is not
    let wide = vec!["z".repeat(MAX_ARG_BYTES); MAX_ARGS_TOTAL / MAX_ARG_BYTES + 1].join(" ");
    assert_eq!(split_args(&wide), Err(ArgError::TooLong));
    for e in [
        ArgError::OpenQuote,
        ArgError::TooMany,
        ArgError::TooLong,
        ArgError::Control,
    ] {
        assert!(!e.describe().is_empty());
    }
}

#[test]
fn hostile_text_never_breaks_the_splitter() {
    // a fixed seed, so a failure can be run again
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let alphabet: Vec<char> = "ab \"\t\\;|&$`'\n\u{0}é漢".chars().collect();
    for _ in 0..2000 {
        let mut s = String::new();
        for _ in 0..(x % 40) {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            s.push(alphabet[(x % alphabet.len() as u64) as usize]);
        }
        if let Ok(words) = split_args(&s) {
            assert!(words.len() <= MAX_ARGS);
            assert!(words.iter().all(|w| w.len() <= MAX_ARG_BYTES));
            assert!(words.iter().all(|w| !w.chars().any(char::is_control)));
        }
    }
}

#[test]
fn the_program_beside_the_window_is_found_and_else_the_name_goes_to_the_path() {
    let dir = temp("beside");
    let window = dir.join(format!("phoenix-pnp-gui{}", std::env::consts::EXE_SUFFIX));
    // nothing beside the window: the bare name
    let bare = program_beside(Some(&window));
    assert_eq!(
        bare,
        PathBuf::from(format!("phoenix-pnp{}", std::env::consts::EXE_SUFFIX))
    );
    assert_eq!(program_beside(None), bare);
    let there = dir.join(format!("phoenix-pnp{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&there, b"not a program").unwrap();
    assert_eq!(program_beside(Some(&window)), there);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_check_on_the_example_profile_says_what_it_holds_and_exits_zero() {
    let out = run_blocking(
        &program(),
        Command::DriverCheck,
        Lang::En,
        &[example("driver_marlin.cfg")],
        LIMIT,
    );
    assert_eq!(out.code, Some(0), "{}", out.text);
    assert!(out.text.contains("evidence=simulator"), "{}", out.text);
    assert!(!out.timed_out && !out.truncated);
    // in another language the same command answers in it
    let es = run_blocking(
        &program(),
        Command::DriverCheck,
        Lang::Es,
        &[example("driver_marlin.cfg")],
        LIMIT,
    );
    assert_eq!(es.code, Some(0));
    assert!(es.text.contains("perfil"), "{}", es.text);
}

#[test]
fn a_command_that_fails_gives_its_code_and_what_it_said() {
    let out = run_blocking(
        &program(),
        Command::DriverCheck,
        Lang::En,
        &["no_such_profile.cfg".to_string()],
        LIMIT,
    );
    assert_eq!(out.code, Some(1));
    assert!(out.text.contains("no_such_profile.cfg"), "{}", out.text);
}

#[test]
fn the_other_commands_run_through_a_panel_the_same_way() {
    let safe = run_blocking(
        &program(),
        Command::SafetyReplay,
        Lang::En,
        &[example("safety_events.txt")],
        LIMIT,
    );
    assert!(safe.code.is_some(), "{}", safe.text);
    assert!(!safe.text.is_empty());
    let home = run_blocking(
        &program(),
        Command::HomingRepeat,
        Lang::En,
        &[
            example("driver_marlin.cfg"),
            "--runs".into(),
            "5".into(),
            "--seed".into(),
            "3".into(),
        ],
        LIMIT,
    );
    assert_eq!(home.code, Some(0), "{}", home.text);
    assert!(home.text.contains("evidence=simulator"));
    let bring = run_blocking(
        &program(),
        Command::Bringup,
        Lang::En,
        &[example("driver_klipper.cfg")],
        LIMIT,
    );
    assert_eq!(bring.code, Some(0), "{}", bring.text);
    assert!(bring.text.contains("PASS  connect"), "{}", bring.text);
    let vac = run_blocking(
        &program(),
        Command::VacuumCalibrate,
        Lang::En,
        &[example("vacuum_readings.csv")],
        LIMIT,
    );
    assert_eq!(vac.code, Some(0), "{}", vac.text);
    let align = run_blocking(
        &program(),
        Command::AlignIterate,
        Lang::En,
        &["--seed".into(), "2".into()],
        LIMIT,
    );
    assert_eq!(align.code, Some(0), "{}", align.text);
    let imp = run_blocking(
        &program(),
        Command::ImportOpenpnp,
        Lang::En,
        &[
            "--packages".into(),
            example("openpnp_package.xml"),
            "--parts".into(),
            example("openpnp_parts.xml"),
        ],
        LIMIT,
    );
    assert!(imp.code.is_some(), "{}", imp.text);
    assert!(imp.text.contains("packages"), "{}", imp.text);
}

#[test]
fn test_board_gen_writes_new_files_and_never_over_one_that_exists() {
    let dir = temp("gen");
    let parts = dir.join("parts.csv");
    let feeders = dir.join("feeders.csv");
    let args = vec![
        "grid".to_string(),
        parts.display().to_string(),
        feeders.display().to_string(),
        "--seed".into(),
        "4".into(),
        "--count".into(),
        "12".into(),
    ];
    let first = run_blocking(&program(), Command::TestBoardGen, Lang::En, &args, LIMIT);
    assert_eq!(first.code, Some(0), "{}", first.text);
    assert!(parts.is_file() && feeders.is_file());
    let before = std::fs::read(&parts).unwrap();
    let second = run_blocking(&program(), Command::TestBoardGen, Lang::En, &args, LIMIT);
    assert_eq!(second.code, Some(1), "{}", second.text);
    assert_eq!(std::fs::read(&parts).unwrap(), before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_program_that_is_not_there_is_told_and_nothing_crashes() {
    let out = run_blocking(
        Path::new("no_such_program_here"),
        Command::Bringup,
        Lang::En,
        &[],
        LIMIT,
    );
    assert!(out.failed_to_start.is_some());
    assert_eq!(out.code, None);
    assert!(!out.timed_out);
}

#[test]
fn a_command_that_takes_longer_than_the_time_is_stopped() {
    // a time of nothing is always over before the program is looked at
    let started = Instant::now();
    let out = run_blocking(
        &program(),
        Command::DriverCheck,
        Lang::En,
        &[example("driver_marlin.cfg")],
        Duration::ZERO,
    );
    assert!(out.timed_out);
    assert_eq!(out.code, None);
    assert!(started.elapsed() < Duration::from_secs(30));
}

#[test]
fn what_a_command_prints_is_cut_at_the_limit_and_the_rest_is_read_and_thrown_away() {
    // ten thousand bytes through a pipe that keeps a thousand: the rest is read, so the program that writes never waits
    let (kept, cut) = read_capped(std::io::repeat(b'x').take(10_000), 1000);
    assert_eq!(kept.len(), 1000);
    assert!(cut);
    let (kept, cut) = read_capped(std::io::repeat(b'y').take(1000), 1000);
    assert_eq!((kept.len(), cut), (1000, false));
    let (kept, cut) = read_capped(std::io::empty(), 1000);
    assert_eq!((kept.len(), cut), (0, false));
    let (kept, cut) = read_capped(std::io::repeat(b'z').take(50), 0);
    assert_eq!((kept.len(), cut), (0, true));
    // a real command stays far under the limit
    let out = run_blocking(
        &program(),
        Command::DriverCheck,
        Lang::En,
        &[example("driver_marlin.cfg")],
        LIMIT,
    );
    assert!(out.text.len() < MAX_OUTPUT_BYTES);
    assert!(!out.truncated);
}

#[test]
fn a_panel_runs_in_its_own_thread_and_takes_the_outcome_when_it_is_there() {
    let mut p = CmdPanel::new(Command::DriverCheck);
    p.args = format!("\"{}\"", example("driver_marlin.cfg"));
    assert!(!p.is_running());
    assert!(p.start(program(), Lang::En, LIMIT));
    assert!(p.is_running());
    // a second start while one is going on does nothing
    assert!(!p.start(program(), Lang::En, LIMIT));
    let until = Instant::now() + Duration::from_secs(60);
    while p.is_running() && Instant::now() < until {
        p.poll();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!p.is_running());
    let out = p.last.as_ref().expect("an outcome");
    assert_eq!(out.code, Some(0), "{}", out.text);
}

#[test]
fn a_panel_with_wrong_arguments_does_not_start_and_says_why() {
    let mut p = CmdPanel::new(Command::Bringup);
    p.args = "\"never closed".to_string();
    assert!(!p.start(program(), Lang::En, LIMIT));
    assert_eq!(p.refused, Some(ArgError::OpenQuote));
    assert!(!p.is_running());
    assert!(p.last.is_none());
    p.args = "fine.cfg".to_string();
    assert!(p.start(program(), Lang::En, LIMIT));
    assert_eq!(p.refused, None);
    let until = Instant::now() + Duration::from_secs(60);
    while p.is_running() && Instant::now() < until {
        p.poll();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(p.last.as_ref().unwrap().code, Some(1));
}

#[test]
fn the_box_holds_a_panel_for_each_command() {
    let mut b = CommandBox::default();
    assert_eq!(b.panels.len(), 15);
    assert!(!b.any_running());
    assert!(!b.poll());
    for c in Command::ALL {
        assert!(b.panel_mut(c).is_some());
    }
}
