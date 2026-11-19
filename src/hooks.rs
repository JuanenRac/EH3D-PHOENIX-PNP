// EH3D-PHOENIX-PNP - hooks: programs that are run when something happens in a job, off by default.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! An operator wants to be told when a job is done, when a part was put aside or when a feeder was switched off: a light, a sound, a message. A hook is a program that is run for an event. Running a
//! program is the riskiest thing this crate can do, so it is done with care and only when it is asked for:
//!
//! * It is off until `hooks` is on. Without it `simulate --hooks FILE` refuses; `--hooks-dry-run` only lists what would be run and runs nothing.
//! * The rules are a file (`event,program,argument...`, see `examples/hooks.csv`). The event is one of `job-finished`, `job-paused`, `job-stopped`, `part-aside` and `feeder-off`.
//!   The program is **the name of a file that lies in the folder of the rules file**, made of letters, digits, `.`, `_` and `-`: no path, no search of the system, no shell, nothing from a
//!   folder above, no symbolic link out of the folder. A batch file (`.bat`, `.cmd`) is allowed, with a warning before the job: it runs commands of the system, with the arguments of its
//!   rule, and what a person writes in it is not looked at here. The rules file is the list of what may be run.
//! * The arguments are passed one by one, never through a shell, so nothing in a reference or in a reason can become a command. `{event}`, `{reference}`, `{feeder}`, `{reason}`, `{phase}`,
//!   `{placed}` and `{total}` stand for the data of the event when they are the whole argument or part of it; at most 8 arguments of 256 characters, without control characters.
//! * A rules file has a size limit and at most 50 rules; at most `hooks_max_runs` programs are run in one job; each one gets `hooks_timeout_s` seconds and is stopped when it takes longer; what
//!   it prints is cut to 4096 bytes. A hook that fails, is not found or is stopped is told and changes nothing about the job.
//!
//! Nothing here moves a machine, and the programs are the operator's own: the crate does not ship any.

use crate::config::MachineConfig;
use crate::error::{Code, PnpError};
use crate::job::{Job, Phase, Status};
use crate::parts::read_records_numbered;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The events a hook can be run for.
pub const EVENTS: [&str; 5] = [
    "job-finished",
    "job-paused",
    "job-stopped",
    "part-aside",
    "feeder-off",
];
/// The largest rules file and the most rules in it.
pub const MAX_BYTES: usize = 65_536;
pub const MAX_RULES: usize = 50;
/// The most arguments of a rule and the longest argument.
pub const MAX_ARGS: usize = 8;
pub const MAX_ARG_CHARS: usize = 256;
/// What a hook prints is cut to this many bytes.
pub const MAX_OUTPUT: usize = 4096;

fn bad(detail: impl Into<String>) -> PnpError {
    PnpError::new(Code::UnsafeSetting, detail)
}

/// Whether hooks may be run: off until `hooks` is on. The reason in words when it is off.
pub fn allowed(cfg: &MachineConfig) -> Result<(), String> {
    if cfg.hooks {
        Ok(())
    } else {
        Err("running hooks is off: set hooks = on in the configuration (--hooks-dry-run lists what would be run)".to_string())
    }
}

/// One rule: for an event, run a program with these arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub event: String,
    pub program: String,
    pub args: Vec<String>,
}

/// Whether a program name is allowed: letters, digits, `.`, `_` and `-` (not starting with `-` or `.`), at most 64 characters. A batch file is allowed (see `is_batch`).
pub fn program_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with(['-', '.'])
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Whether a program is a batch file of Windows (`.bat` or `.cmd`, in any case): a program that runs commands of the system.
pub fn is_batch(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".bat") || lower.ends_with(".cmd")
}

/// The warnings for a list of rules: one for each batch file that a rule names, told once, to be shown before the job runs.
pub fn warnings(rules: &[Rule]) -> Vec<String> {
    let mut seen: Vec<&str> = Vec::new();
    let mut out = Vec::new();
    for r in rules {
        if is_batch(&r.program) && !seen.contains(&r.program.as_str()) {
            seen.push(&r.program);
            out.push(format!(
                "warning: '{}' is a batch file: it runs commands of the system with the arguments of its rule, so read it before a job runs it",
                r.program
            ));
        }
    }
    out
}

/// Reads the rules: `event,program,argument...`, one rule for each, with an optional header (`event,program,...`) and comment lines (`#`). A rule that is not valid is an error with its line.
pub fn parse_rules(text: &str) -> Result<Vec<Rule>, PnpError> {
    if text.len() > MAX_BYTES {
        return Err(bad(format!(
            "the rules of the hooks are bigger than {MAX_BYTES} bytes"
        )));
    }
    let text = text.trim_start_matches('\u{feff}');
    let kept: String = text
        .lines()
        .map(|l| {
            if l.trim_start().starts_with('#') {
                ""
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let records =
        read_records_numbered(&kept, ',').map_err(|(line, e)| bad(format!("line {line}: {e}")))?;
    let mut rules: Vec<Rule> = Vec::new();
    for (i, (line, record)) in records.into_iter().enumerate() {
        if i == 0 && record.first().is_some_and(|c| c == "event") {
            continue;
        }
        if record.len() < 2 {
            return Err(bad(format!(
                "line {line}: a rule needs an event and a program (event,program,argument...)"
            )));
        }
        let event = record[0].as_str();
        if !EVENTS.contains(&event) {
            return Err(bad(format!(
                "line {line}: '{event}' is not an event (one of {})",
                EVENTS.join(", ")
            )));
        }
        let program = record[1].as_str();
        if !program_ok(program) {
            return Err(bad(format!(
                "line {line}: '{program}' is not a program that may be run: a file in the folder of the rules, named with letters, digits, '.', '_' and '-' only"
            )));
        }
        let args: Vec<String> = record[2..].to_vec();
        if args.len() > MAX_ARGS {
            return Err(bad(format!("line {line}: more than {MAX_ARGS} arguments")));
        }
        for a in &args {
            if a.chars().count() > MAX_ARG_CHARS || a.chars().any(char::is_control) {
                return Err(bad(format!(
                    "line {line}: an argument is longer than {MAX_ARG_CHARS} characters or has a control character"
                )));
            }
        }
        if rules.len() >= MAX_RULES {
            return Err(bad(format!("line {line}: more than {MAX_RULES} rules")));
        }
        rules.push(Rule {
            event: event.to_string(),
            program: program.to_string(),
            args,
        });
    }
    Ok(rules)
}

/// Something that happened in a job.
#[derive(Debug, Clone, PartialEq)]
pub struct HookEvent {
    pub name: &'static str,
    pub reference: String,
    pub feeder: String,
    pub reason: String,
    pub phase: &'static str,
    pub placed: usize,
    pub total: usize,
}

/// The events of a job as it stands: a `part-aside` for each part put aside, a `feeder-off` for each feeder that was switched off, and last the event of the phase of the job (`job-finished`,
/// `job-paused` or `job-stopped`; a job that is ready has none).
pub fn events_of(job: &Job) -> Vec<HookEvent> {
    let phase = crate::exchange::phase_name(job.phase());
    let total = job.status().len();
    let placed = job
        .status()
        .iter()
        .filter(|s| **s == Status::Placed)
        .count();
    let base = |name: &'static str| HookEvent {
        name,
        reference: String::new(),
        feeder: String::new(),
        reason: String::new(),
        phase,
        placed,
        total,
    };
    let mut out = Vec::new();
    for (i, p) in job.plan().plan.placements.iter().enumerate() {
        if job.status()[i] == Status::Deferred {
            out.push(HookEvent {
                reference: p.reference.clone(),
                feeder: p.feeder.clone(),
                reason: job.reason(i).to_string(),
                ..base("part-aside")
            });
        }
    }
    for f in job.stats().feeders.iter().filter(|f| f.disabled) {
        out.push(HookEvent {
            feeder: f.id.clone(),
            ..base("feeder-off")
        });
    }
    match job.phase() {
        Phase::Finished => out.push(base("job-finished")),
        Phase::Paused => out.push(base("job-paused")),
        Phase::Stopped => out.push(base("job-stopped")),
        Phase::Ready => {}
    }
    out
}

/// A program to run: its name, its arguments with the data of the event put in, and the event.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub event: &'static str,
    pub program: String,
    pub args: Vec<String>,
}

impl Call {
    /// The call as a line for a person.
    pub fn line(&self) -> String {
        let mut s = format!("{}: {}", self.event, self.program);
        for a in &self.args {
            s.push(' ');
            s.push_str(&format!("{a:?}"));
        }
        s
    }
}

/// The argument with the data of the event put in. It is done in one pass, so that data which looks like a name between brackets is not filled in again.
fn fill(arg: &str, e: &HookEvent) -> String {
    let names: [(&str, String); 7] = [
        ("{event}", e.name.to_string()),
        ("{reference}", e.reference.clone()),
        ("{feeder}", e.feeder.clone()),
        ("{reason}", e.reason.clone()),
        ("{phase}", e.phase.to_string()),
        ("{placed}", e.placed.to_string()),
        ("{total}", e.total.to_string()),
    ];
    let mut out = String::new();
    let mut rest = arg;
    while let Some(at) = rest.find('{') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        match names.iter().find(|(k, _)| tail.starts_with(k)) {
            Some((k, v)) => {
                out.push_str(v);
                rest = &tail[k.len()..];
            }
            None => {
                out.push('{');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The calls that the rules ask for the events, in the order of the events and, for an event, of the rules; no more than `max_runs`. Gives the calls and how many were left out.
pub fn calls_for(rules: &[Rule], events: &[HookEvent], max_runs: usize) -> (Vec<Call>, usize) {
    let mut calls = Vec::new();
    let mut left_out = 0;
    for e in events {
        for r in rules.iter().filter(|r| r.event == e.name) {
            if calls.len() >= max_runs {
                left_out += 1;
                continue;
            }
            calls.push(Call {
                event: e.name,
                program: r.program.clone(),
                args: r.args.iter().map(|a| fill(a, e)).collect(),
            });
        }
    }
    (calls, left_out)
}

/// What came of a call.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// The program ran and ended with this exit code (-1 when it ended without one), printing this (cut).
    Ran { code: i32, output: String },
    /// The program was stopped after the time it was given.
    TimedOut,
    /// The program could not be run, with the reason.
    Failed(String),
}

impl Outcome {
    pub fn ok(&self) -> bool {
        matches!(self, Outcome::Ran { code: 0, .. })
    }
}

/// The path of a program: a regular file that lies in `folder`, found by its name, and still in `folder` once the links are followed.
pub fn resolve(folder: &Path, program: &str) -> Result<PathBuf, String> {
    if !program_ok(program) {
        return Err(format!("'{program}' is not a program that may be run"));
    }
    let root = folder
        .canonicalize()
        .map_err(|e| format!("{}: {e}", folder.display()))?;
    let path = root.join(program);
    let real = path.canonicalize().map_err(|_| {
        format!(
            "{program} is not in the folder of the rules ({})",
            root.display()
        )
    })?;
    if real.parent() != Some(root.as_path()) {
        return Err(format!("{program} leads out of the folder of the rules"));
    }
    if !real.is_file() {
        return Err(format!("{program} is not a file"));
    }
    Ok(real)
}

fn cut(bytes: &[u8]) -> String {
    let shown = &bytes[..bytes.len().min(MAX_OUTPUT)];
    let mut s = String::from_utf8_lossy(shown).into_owned();
    if bytes.len() > MAX_OUTPUT {
        s.push_str("...");
    }
    s.trim_end().to_string()
}

/// Runs one call: the program of `folder` with the arguments, through no shell, waiting at most `timeout_s` seconds, with what it prints cut to `MAX_OUTPUT` bytes. The event is also in the
/// environment (`PHOENIX_PNP_EVENT`).
pub fn run_call(folder: &Path, call: &Call, timeout_s: f64) -> Outcome {
    let path = match resolve(folder, &call.program) {
        Ok(p) => p,
        Err(e) => return Outcome::Failed(e),
    };
    let mut cmd = Command::new(&path);
    cmd.args(&call.args)
        .env("PHOENIX_PNP_EVENT", call.event)
        .current_dir(folder)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // no window of its own
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return Outcome::Failed(format!("{}: {e}", call.program)),
    };
    // what it prints is read on threads, so that a program that prints a lot cannot fill the pipe and wait for ever
    let reader = |pipe: Option<Box<dyn std::io::Read + Send>>| {
        std::thread::spawn(move || {
            let mut kept: Vec<u8> = Vec::new();
            if let Some(mut p) = pipe {
                let mut buf = [0u8; 1024];
                while let Ok(n) = p.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    if kept.len() <= MAX_OUTPUT {
                        kept.extend_from_slice(&buf[..n]);
                    }
                }
            }
            kept
        })
    };
    let out = reader(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
    );
    let err = reader(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
    );
    let started = Instant::now();
    let limit = Duration::from_secs_f64(timeout_s.clamp(0.0, 3600.0));
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {}
            Err(e) => return Outcome::Failed(format!("{}: {e}", call.program)),
        }
        if started.elapsed() >= limit {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let mut printed = out.join().unwrap_or_default();
    printed.extend(err.join().unwrap_or_default());
    match status {
        Some(s) => Outcome::Ran {
            code: s.code().unwrap_or(-1),
            output: cut(&printed),
        },
        None => Outcome::TimedOut,
    }
}
