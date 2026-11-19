//! Hooks, the programs that are run for the events of a job: off by default, the rules and what a program may be, the events of a job, the calls with the data filled in, the running with a
//! time limit and a limit of what is printed, and `simulate --hooks`.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::hooks::{
    allowed, calls_for, events_of, parse_rules, program_ok, resolve, run_call, Call, HookEvent,
    Outcome, Rule, EVENTS, MAX_ARGS, MAX_BYTES, MAX_OUTPUT, MAX_RULES,
};
use eh3d_phoenix_pnp::job::Job;
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const MACHINE: &str = include_str!("../examples/machine.cfg");
const EXAMPLE: &str = include_str!("../examples/hooks.csv");
const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nR3,10k,R,30,10,180,top\nC1,100n,C,10,20,0,top\nC2,100n,C,20,20,0,top\nU1,MCU,Q,40,15,0,top\n";
const FEEDERS: &str =
    "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\n";

fn job(cfg: &str) -> Job {
    let cfg = MachineConfig::parse(cfg).unwrap();
    let parts = from_csv(PARTS).unwrap();
    let feeders = feeding::from_csv(FEEDERS).unwrap();
    let picks = assign(&parts, &feeders, Some(Side::Top));
    let plan = plan_advanced(
        &parts,
        &feeders,
        &picks,
        &Catalog::new(),
        &cfg,
        &Options::default(),
    )
    .unwrap();
    Job::new(plan, feeders, cfg).unwrap()
}

fn refused(text: &str) -> String {
    let e = parse_rules(text).unwrap_err();
    assert_eq!(e.code, Code::UnsafeSetting);
    e.detail
}

fn names(events: &[HookEvent]) -> Vec<&'static str> {
    events.iter().map(|e| e.name).collect()
}

// ---- off by default

#[test]
fn hooks_are_off_by_default_and_their_settings_have_ranges() {
    let d = MachineConfig::default();
    assert!(!d.hooks);
    assert_eq!((d.hooks_timeout_s, d.hooks_max_runs), (5.0, 20));
    assert!(allowed(&d).unwrap_err().contains("hooks = on"));
    assert!(allowed(&MachineConfig::parse("hooks = on").unwrap()).is_ok());
    assert!(!MachineConfig::parse(MACHINE).unwrap().hooks);
    for key in ["hooks", "hooks_timeout_s", "hooks_max_runs"] {
        assert!(KEYS.contains(&key), "{key}");
        assert!(MACHINE.contains(&format!("\n{key} = ")), "{key}");
    }
    for (key, ok, out) in [
        ("hooks_timeout_s", ["1", "60"], ["0.5", "61"]),
        ("hooks_max_runs", ["1", "100"], ["0", "101"]),
    ] {
        for v in ok {
            assert!(
                MachineConfig::parse(&format!("{key} = {v}")).is_ok(),
                "{key} {v}"
            );
        }
        for v in out {
            assert_eq!(
                MachineConfig::parse(&format!("{key} = {v}"))
                    .unwrap_err()
                    .code,
                Code::ConfigOutOfRange,
                "{key} {v}"
            );
        }
    }
    assert!(MachineConfig::parse("hooks = maybe").is_err());
}

// ---- the rules

#[test]
fn the_example_rules_are_read() {
    let r = parse_rules(EXAMPLE).unwrap();
    assert_eq!(r.len(), 4);
    assert_eq!(r[0].event, "job-finished");
    assert_eq!(r[0].program, "notify");
    assert_eq!(r[0].args, ["{event}", "{placed}", "{total}"]);
    assert_eq!(r[2].args, ["{event}", "{reference}", "{reason}"]);
    for rule in &r {
        assert!(EVENTS.contains(&rule.event.as_str()));
    }
    // without the header, with a byte order mark and quotes
    let r = parse_rules("\u{feff}feeder-off,a.exe,\"two, words\"\n").unwrap();
    assert_eq!(r[0].args, ["two, words"]);
    assert!(parse_rules("").unwrap().is_empty());
    assert!(parse_rules("# only a comment\n").unwrap().is_empty());
}

#[test]
fn a_rule_that_is_not_safe_is_refused_with_its_line() {
    for (text, want) in [
        (
            "job-finished\n",
            "line 1: a rule needs an event and a program",
        ),
        ("job-done,notify\n", "'job-done' is not an event"),
        ("job-finished,../notify\n", "not a program that may be run"),
        ("job-finished,a/b\n", "not a program that may be run"),
        ("job-finished,a\\b\n", "not a program that may be run"),
        ("job-finished,C:notify\n", "not a program that may be run"),
        ("job-finished,a b\n", "not a program that may be run"),
        ("job-finished,-rf\n", "not a program that may be run"),
        ("job-finished,.hidden\n", "not a program that may be run"),
        ("job-finished,x.bat\n", "not a program that may be run"),
        ("job-finished,X.CMD\n", "not a program that may be run"),
        ("job-finished,\"\"\n", "not a program that may be run"),
        ("job-finished,a;b\n", "not a program that may be run"),
        ("job-finished,$(ls)\n", "not a program that may be run"),
        ("\"job-finished\n", "line"),
    ] {
        let d = refused(text);
        assert!(d.contains(want), "{text:?}: {d}");
    }
    assert!(!program_ok(&"a".repeat(65)));
    assert!(program_ok(&"a".repeat(64)));
    for ok in ["notify", "notify.exe", "my_hook-2.sh", "a"] {
        assert!(program_ok(ok), "{ok}");
    }
    // the arguments
    let nine = format!("job-finished,p{}\n", ",x".repeat(MAX_ARGS + 1));
    assert!(refused(&nine).contains("more than 8 arguments"));
    assert!(parse_rules(&format!("job-finished,p{}\n", ",x".repeat(MAX_ARGS))).is_ok());
    let long = format!("job-finished,p,{}\n", "a".repeat(257));
    assert!(refused(&long).contains("longer than 256"));
    assert!(refused("job-finished,p,\"a\u{7}b\"\n").contains("control character"));
    // the number of rules and the size of the file
    let many = "job-finished,p\n".repeat(MAX_RULES + 1);
    assert!(refused(&many).contains("more than 50 rules"));
    assert_eq!(
        parse_rules(&"job-finished,p\n".repeat(MAX_RULES))
            .unwrap()
            .len(),
        MAX_RULES
    );
    let big = format!("# {}\n", "x".repeat(MAX_BYTES));
    assert!(refused(&big).contains("bigger than"));
}

// ---- the events of a job

#[test]
fn a_job_has_the_events_of_what_happened_in_it() {
    // a job that has not started has none
    assert!(events_of(&job("")).is_empty());
    // one that finished
    let mut j = job("");
    j.run(None).unwrap();
    let e = events_of(&j);
    assert_eq!(names(&e), ["job-finished"]);
    assert_eq!((e[0].placed, e[0].total, e[0].phase), (6, 6, "finished"));
    // one that is paused
    let mut p = job("");
    p.run(Some(3)).unwrap();
    let e = events_of(&p);
    assert_eq!(names(&e), ["job-paused"]);
    assert_eq!(e[0].phase, "paused");
    // one that an error stopped
    let mut s =
        job("error_policy = alert\nsim_pick_fail_rate = 1\npick_retries = 0\nfault_limit = 0\n");
    assert!(s.run(None).is_err());
    assert_eq!(names(&events_of(&s)), ["job-stopped"]);
    // parts put aside and a feeder switched off
    let mut a =
        job("error_policy = defer\nsim_pick_fail_rate = 1\npick_retries = 0\nfault_limit = 1\n");
    a.run(None).unwrap();
    let e = events_of(&a);
    let aside: Vec<&HookEvent> = e.iter().filter(|x| x.name == "part-aside").collect();
    assert_eq!(aside.len(), 6);
    assert!(aside
        .iter()
        .all(|x| !x.reference.is_empty() && !x.feeder.is_empty() && !x.reason.is_empty()));
    assert!(e
        .iter()
        .any(|x| x.name == "feeder-off" && !x.feeder.is_empty()));
    assert_eq!(e.last().unwrap().name, "job-finished");
    // the events of the parts come first, the one of the job last
    let first_job = e.iter().position(|x| x.name == "job-finished").unwrap();
    assert_eq!(first_job, e.len() - 1);
}

// ---- the calls

fn rule(event: &str, program: &str, args: &[&str]) -> Rule {
    Rule {
        event: event.to_string(),
        program: program.to_string(),
        args: args.iter().map(ToString::to_string).collect(),
    }
}

fn event(name: &'static str, reference: &str, reason: &str) -> HookEvent {
    HookEvent {
        name,
        reference: reference.to_string(),
        feeder: "F1".to_string(),
        reason: reason.to_string(),
        phase: "finished",
        placed: 4,
        total: 6,
    }
}

#[test]
fn the_data_is_filled_in_once_and_the_calls_are_limited() {
    let rules = [
        rule(
            "part-aside",
            "note",
            &[
                "{event}:{reference}",
                "{reason}",
                "x{feeder}y",
                "{nothing}",
                "{",
            ],
        ),
        rule("job-finished", "done", &["{placed}/{total}", "{phase}"]),
    ];
    let events = [
        event("part-aside", "R1", "it fell {phase} {event}"),
        event("job-finished", "", ""),
    ];
    let (calls, left) = calls_for(&rules, &events, 20);
    assert_eq!(left, 0);
    assert_eq!(
        calls,
        vec![
            Call {
                event: "part-aside",
                program: "note".to_string(),
                args: vec![
                    "part-aside:R1".to_string(),
                    "it fell {phase} {event}".to_string(),
                    "xF1y".to_string(),
                    "{nothing}".to_string(),
                    "{".to_string()
                ]
            },
            Call {
                event: "job-finished",
                program: "done".to_string(),
                args: vec!["4/6".to_string(), "finished".to_string()]
            }
        ]
    );
    assert!(calls[0]
        .line()
        .starts_with("part-aside: note \"part-aside:R1\""));
    // an event with no rule has no call, and the number of calls is limited
    let (none, left) = calls_for(&rules, &[event("feeder-off", "", "")], 20);
    assert!(none.is_empty() && left == 0);
    let many: Vec<HookEvent> = (0..30).map(|_| event("job-finished", "", "")).collect();
    let (calls, left) = calls_for(&rules, &many, 7);
    assert_eq!((calls.len(), left), (7, 23));
    let (calls, left) = calls_for(&rules, &many, 0);
    assert_eq!((calls.len(), left), (0, 30));
}

// ---- finding and running a program

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("phoenix_pnp_hooks_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// A copy of the program itself in `dir`, to stand for a program of the operator.
fn plant(dir: &Path, name: &str) -> PathBuf {
    let target = dir.join(name);
    std::fs::copy(env!("CARGO_BIN_EXE_phoenix-pnp"), &target).unwrap();
    target
}

const HOOK: &str = if cfg!(windows) {
    "notify.exe"
} else {
    "notify"
};

#[test]
fn only_a_file_of_the_folder_is_found() {
    let dir = scratch("resolve");
    plant(&dir, HOOK);
    let found = resolve(&dir, HOOK).unwrap();
    assert!(found.is_file() && found.ends_with(HOOK));
    assert!(resolve(&dir, "missing")
        .unwrap_err()
        .contains("not in the folder"));
    for name in ["../notify", "a/b", "-x", "x.bat", ""] {
        assert!(
            resolve(&dir, name).unwrap_err().contains("may be run"),
            "{name}"
        );
    }
    std::fs::create_dir_all(dir.join("folder")).unwrap();
    assert!(resolve(&dir, "folder").unwrap_err().contains("not a file"));
    assert!(resolve(&dir.join("nothing"), HOOK).is_err());
    // a link that leads out of the folder is not followed out
    #[cfg(unix)]
    {
        let outside = scratch("outside");
        plant(&outside, "other");
        std::os::unix::fs::symlink(outside.join("other"), dir.join("linked")).unwrap();
        assert!(resolve(&dir, "linked").unwrap_err().contains("leads out"));
        let _ = std::fs::remove_dir_all(&outside);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

fn call(program: &str, args: &[&str]) -> Call {
    Call {
        event: "job-finished",
        program: program.to_string(),
        args: args.iter().map(ToString::to_string).collect(),
    }
}

#[test]
fn a_program_is_run_with_its_arguments_and_what_it_prints_is_cut() {
    let dir = scratch("run");
    plant(&dir, HOOK);
    match run_call(&dir, &call(HOOK, &["help"]), 20.0) {
        Outcome::Ran { code, output } => {
            assert_eq!(code, 0);
            assert!(
                output.contains("phoenix-pnp: pick and place assembly"),
                "{output}"
            );
            // the help is longer than what is kept
            assert!(output.len() <= MAX_OUTPUT + 8, "{}", output.len());
            assert!(output.ends_with("..."), "{output}");
        }
        other => panic!("{other:?}"),
    }
    // a program that ends with an error is told with its code and its message
    match run_call(&dir, &call(HOOK, &["no-such-command"]), 20.0) {
        Outcome::Ran { code, output } => {
            assert_ne!(code, 0);
            assert!(!output.is_empty());
        }
        other => panic!("{other:?}"),
    }
    assert!(run_call(&dir, &call(HOOK, &["help"]), 20.0).ok());
    assert!(!run_call(&dir, &call(HOOK, &["no-such-command"]), 20.0).ok());
    // a program that is not there, or not allowed, is not run
    assert!(matches!(
        run_call(&dir, &call("missing", &[]), 5.0),
        Outcome::Failed(_)
    ));
    assert!(matches!(
        run_call(&dir, &call("../notify", &[]), 5.0),
        Outcome::Failed(_)
    ));
    // an argument is one argument: a shell word in it is only text
    match run_call(&dir, &call(HOOK, &["help; echo hacked", "&& whoami"]), 20.0) {
        Outcome::Ran { output, .. } => assert!(!output.contains("hacked\n"), "{output}"),
        other => panic!("{other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
fn write_script(path: &Path, text: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, text).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(not(unix))]
fn write_script(_path: &Path, _text: &str) {}

#[test]
fn a_program_that_takes_too_long_is_stopped() {
    let dir = scratch("slow");
    // a program that waits: sleep, or ping, copied to the folder
    let (name, args): (&str, Vec<&str>) = if cfg!(windows) {
        ("waiter.exe", vec!["-n", "30", "127.0.0.1"])
    } else {
        ("waiter", vec![])
    };
    if cfg!(windows) {
        let source = r"C:\Windows\System32\ping.exe";
        if !Path::new(source).exists() {
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        std::fs::copy(source, dir.join(name)).unwrap();
    } else {
        // a script, because a copy of a multi-call binary does not know its new name
        // exec: the sleep takes the place of the shell, so that the stop reaches it and not a child that keeps the output open
        write_script(&dir.join(name), "#!/bin/sh\nexec sleep 30\n");
    }
    let started = std::time::Instant::now();
    let outcome = run_call(&dir, &call(name, &args), 0.5);
    assert_eq!(outcome, Outcome::TimedOut);
    assert!(started.elapsed().as_secs_f64() < 10.0);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- simulate --hooks

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

#[test]
fn simulate_runs_hooks_only_when_they_are_turned_on() {
    let dir = scratch("sim");
    plant(&dir, HOOK);
    let rules = dir.join("hooks.csv");
    std::fs::write(
        &rules,
        format!(
            "job-finished,{HOOK},help\njob-finished,missing,{{event}}\nfeeder-off,{HOOK},help\n"
        ),
    )
    .unwrap();
    let base = ["simulate", "examples/board.pos", "examples/feeders.csv"];
    let go = |extra: &[&str]| -> Output {
        let mut v: Vec<&str> = base.to_vec();
        v.extend_from_slice(extra);
        run(&v)
    };
    // off: refused with what to turn on, and the job is not even run
    let o = go(&["--hooks", p(&rules)]);
    assert!(!o.status.success());
    assert!(
        text(&o.stderr).contains("hooks = on"),
        "{}",
        text(&o.stderr)
    );
    assert!(!text(&o.stdout).contains("placed 6 of 6"));
    // a dry run lists the calls and runs nothing
    let o = go(&["--hooks", p(&rules), "--hooks-dry-run"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    assert!(
        out.contains(&format!("hook (not run): job-finished: {HOOK} \"help\"")),
        "{out}"
    );
    assert!(
        out.contains("hook (not run): job-finished: missing \"job-finished\""),
        "{out}"
    );
    assert!(!out.contains("feeder-off"), "{out}");
    // on: the programs are run, one is not found, and the job and the exit code are as they were
    let cfg = dir.join("on.cfg");
    std::fs::write(&cfg, "hooks = on\nhooks_timeout_s = 20\n").unwrap();
    let o = go(&["--hooks", p(&rules), "--config", p(&cfg)]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    assert!(
        out.contains(": ran, it said: phoenix-pnp: pick and place assembly"),
        "{out}"
    );
    assert!(
        out.contains("not run, missing is not in the folder of the rules"),
        "{out}"
    );
    assert!(out.contains("placed 6 of 6"), "{out}");
    // only so many are run
    let one = dir.join("one.cfg");
    std::fs::write(
        &one,
        "hooks = on\nhooks_max_runs = 1\nhooks_timeout_s = 20\n",
    )
    .unwrap();
    let out = text(&go(&["--hooks", p(&rules), "--config", p(&one)]).stdout);
    assert!(
        out.contains("1 more hooks were left out: hooks_max_runs is 1"),
        "{out}"
    );
    // rules that are not safe are refused before the job runs
    let evil = dir.join("evil.csv");
    std::fs::write(&evil, "job-finished,../notify\n").unwrap();
    let o = go(&["--hooks", p(&evil), "--config", p(&cfg)]);
    assert!(!o.status.success());
    assert!(
        text(&o.stderr).contains("PHOENIX-PNP-009"),
        "{}",
        text(&o.stderr)
    );
    assert!(!text(&o.stdout).contains("placed"));
    // without the rules there is nothing to dry-run, and the rules may not be a report
    assert!(!go(&["--hooks-dry-run"]).status.success());
    assert!(!go(&["--hooks", "nothing.csv", "--config", p(&cfg)])
        .status
        .success());
    let help = text(&run(&["help"]).stdout);
    assert!(help.contains("--hooks F") && help.contains("--hooks-dry-run"));
    let _ = std::fs::remove_dir_all(&dir);
}
