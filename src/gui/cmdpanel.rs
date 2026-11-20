// EH3D-PHOENIX-PNP - the panels of the driver and the assistants, without the window.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! Eleven commands of the program (`driver-check`, `driver-test`, `driver-run`, `safety-replay`, `homing-repeat`, `bringup`, `import-openpnp`, `assist`, `align-iterate`,
//! `vacuum-calibrate` and `test-board-gen`) have a panel each in the window. A panel does not repeat what its command does: it runs the command-line program that sits beside the window,
//! with the arguments typed in the panel, and shows what the program printed. So the panel and the command cannot disagree, and everything the command refuses (a file that exists is
//! never written over, a file that is read is never written to, the only controller is a simulated one) is refused here too.
//!
//! What makes it safe: the command is one of the eleven (never a word the person typed), the arguments go to the program one by one (no shell reads them), a quote groups words and
//! nothing else is special, the number and the length of the arguments are bounded, the program runs without a console window and without anything to read from the keyboard, it is
//! stopped when it takes longer than the time in the settings, and what it prints is cut at a limit.

use super::texts::Text;
use crate::error::Lang;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::{Duration, Instant};

/// The most arguments a panel passes, the longest one and the most bytes of all of them together.
pub const MAX_ARGS: usize = 64;
pub const MAX_ARG_BYTES: usize = 1024;
pub const MAX_ARGS_TOTAL: usize = 8192;
/// The most bytes of what the program prints that are kept.
pub const MAX_OUTPUT_BYTES: usize = 262_144;
/// The name of the command-line program.
pub const PROGRAM: &str = "phoenix-pnp";

/// The commands that have a panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    DriverCheck,
    DriverTest,
    DriverRun,
    SafetyReplay,
    HomingRepeat,
    Bringup,
    ImportOpenpnp,
    Assist,
    AlignIterate,
    VacuumCalibrate,
    TestBoardGen,
}

impl Command {
    pub const ALL: [Command; 11] = [
        Command::DriverCheck,
        Command::DriverTest,
        Command::DriverRun,
        Command::SafetyReplay,
        Command::HomingRepeat,
        Command::Bringup,
        Command::ImportOpenpnp,
        Command::Assist,
        Command::AlignIterate,
        Command::VacuumCalibrate,
        Command::TestBoardGen,
    ];

    /// The word of the command on the command line.
    pub fn name(self) -> &'static str {
        match self {
            Command::DriverCheck => "driver-check",
            Command::DriverTest => "driver-test",
            Command::DriverRun => "driver-run",
            Command::SafetyReplay => "safety-replay",
            Command::HomingRepeat => "homing-repeat",
            Command::Bringup => "bringup",
            Command::ImportOpenpnp => "import-openpnp",
            Command::Assist => "assist",
            Command::AlignIterate => "align-iterate",
            Command::VacuumCalibrate => "vacuum-calibrate",
            Command::TestBoardGen => "test-board-gen",
        }
    }

    /// The name of the panel in the frame (letters, digits and `_`).
    pub fn panel_id(self) -> &'static str {
        match self {
            Command::DriverCheck => "cmd_driver_check",
            Command::DriverTest => "cmd_driver_test",
            Command::DriverRun => "cmd_driver_run",
            Command::SafetyReplay => "cmd_safety_replay",
            Command::HomingRepeat => "cmd_homing_repeat",
            Command::Bringup => "cmd_bringup",
            Command::ImportOpenpnp => "cmd_import_openpnp",
            Command::Assist => "cmd_assist",
            Command::AlignIterate => "cmd_align_iterate",
            Command::VacuumCalibrate => "cmd_vacuum_calibrate",
            Command::TestBoardGen => "cmd_test_board_gen",
        }
    }

    /// The command that a panel of the frame belongs to.
    pub fn from_panel(id: &str) -> Option<Command> {
        Command::ALL.into_iter().find(|c| c.panel_id() == id)
    }

    /// The word that names the panel.
    pub fn title(self) -> Text {
        match self {
            Command::DriverCheck => Text::CmdTitleDriverCheck,
            Command::DriverTest => Text::CmdTitleDriverTest,
            Command::DriverRun => Text::CmdTitleDriverRun,
            Command::SafetyReplay => Text::CmdTitleSafetyReplay,
            Command::HomingRepeat => Text::CmdTitleHomingRepeat,
            Command::Bringup => Text::CmdTitleBringup,
            Command::ImportOpenpnp => Text::CmdTitleImportOpenpnp,
            Command::Assist => Text::CmdTitleAssist,
            Command::AlignIterate => Text::CmdTitleAlignIterate,
            Command::VacuumCalibrate => Text::CmdTitleVacuumCalibrate,
            Command::TestBoardGen => Text::CmdTitleTestBoardGen,
        }
    }

    /// The sentence that says what the command wants and does.
    pub fn hint(self) -> Text {
        match self {
            Command::DriverCheck => Text::CmdHintDriverCheck,
            Command::DriverTest => Text::CmdHintDriverTest,
            Command::DriverRun => Text::CmdHintDriverRun,
            Command::SafetyReplay => Text::CmdHintSafetyReplay,
            Command::HomingRepeat => Text::CmdHintHomingRepeat,
            Command::Bringup => Text::CmdHintBringup,
            Command::ImportOpenpnp => Text::CmdHintImportOpenpnp,
            Command::Assist => Text::CmdHintAssist,
            Command::AlignIterate => Text::CmdHintAlignIterate,
            Command::VacuumCalibrate => Text::CmdHintVacuumCalibrate,
            Command::TestBoardGen => Text::CmdHintTestBoardGen,
        }
    }

    /// The form of the arguments, the same in every language (the options of the command line).
    pub fn usage(self) -> &'static str {
        match self {
            Command::DriverCheck => "PROFILE",
            Command::DriverTest => "PROFILE TRANSCRIPT",
            Command::DriverRun => {
                "PARTS FEEDERS PROFILE [--side top|bottom] [--fault error@3] [--seed 1]"
            }
            Command::SafetyReplay => "EVENTS [--profile FILE]",
            Command::HomingRepeat => "PROFILE [--runs 10] [--jitter 0.02] [--max-spread 0.05]",
            Command::Bringup => "PROFILE [--fault busy@2] [--seed 1]",
            Command::ImportOpenpnp => {
                "[--machine FILE] [--packages FILE] [--parts FILE] [--out-dir FOLDER]"
            }
            Command::Assist => {
                "PARTS [--feeders FILE] [--order reel|reference] [--answers FILE] [--out FILE]"
            }
            Command::AlignIterate => {
                "[--error 0.4,-0.3] [--noise 0.01] [--scale 1] [--seed 1] [--blind 2]"
            }
            Command::VacuumCalibrate => "READINGS [--out FILE]",
            Command::TestBoardGen => {
                "grid|mixed|dense PARTS_OUT FEEDERS_OUT [--seed 1] [--count 40] [--pitch 2]"
            }
        }
    }
}

/// Why the arguments typed were refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgError {
    /// A quote that is never closed.
    OpenQuote,
    /// More arguments than the limit.
    TooMany,
    /// An argument or the sum of them is longer than the limit.
    TooLong,
    /// A control character (a line end, a tab inside a word, a code of the terminal).
    Control,
}

impl ArgError {
    /// What went wrong, in English (the window prefixes it with its own words).
    pub fn describe(&self) -> &'static str {
        match self {
            ArgError::OpenQuote => "a quote is not closed",
            ArgError::TooMany => "there are more arguments than the limit",
            ArgError::TooLong => "an argument is too long",
            ArgError::Control => "a control character is not allowed",
        }
    }
}

/// Splits what was typed into the arguments: words separated by spaces, a double quote groups words (`"my folder"`) and is not part of the word, an empty pair of quotes is an empty
/// argument; nothing else is special (a backslash is a backslash, so a Windows path is typed as it is).
pub fn split_args(text: &str) -> Result<Vec<String>, ArgError> {
    let mut args: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quoted = false;
    let mut total = 0usize;
    for c in text.chars() {
        if c.is_control() && c != ' ' && c != '\t' {
            return Err(ArgError::Control);
        }
        if c == '"' {
            quoted = !quoted;
            started = true;
        } else if (c == ' ' || c == '\t') && !quoted {
            if started {
                total += current.len();
                args.push(std::mem::take(&mut current));
                started = false;
            }
        } else {
            if c == '\t' {
                return Err(ArgError::Control);
            }
            current.push(c);
            started = true;
        }
        if current.len() > MAX_ARG_BYTES {
            return Err(ArgError::TooLong);
        }
        if args.len() > MAX_ARGS {
            return Err(ArgError::TooMany);
        }
    }
    if quoted {
        return Err(ArgError::OpenQuote);
    }
    if started {
        total += current.len();
        args.push(current);
    }
    if args.len() > MAX_ARGS {
        return Err(ArgError::TooMany);
    }
    if total > MAX_ARGS_TOTAL {
        return Err(ArgError::TooLong);
    }
    Ok(args)
}

/// The command-line program: the one beside the file `window` when it is there, else the bare name (the system looks it up in the path).
pub fn program_beside(window: Option<&Path>) -> PathBuf {
    let name = format!("{PROGRAM}{}", std::env::consts::EXE_SUFFIX);
    if let Some(dir) = window.and_then(Path::parent) {
        let candidate = dir.join(&name);
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from(name)
}

/// What a run came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// The exit code, or `None` when the program was stopped or could not be started.
    pub code: Option<i32>,
    /// What the program printed (the answer, then what it said on the error output), cut at the limit.
    pub text: String,
    /// Whether it was stopped for taking longer than the time allowed.
    pub timed_out: bool,
    /// Whether the text was cut.
    pub truncated: bool,
    /// Why the program could not be started, when it could not.
    pub failed_to_start: Option<String>,
}

/// Reads a pipe to its end and keeps the first `max` bytes; the rest is read and thrown away, so the program that writes to it never waits for room. Gives the bytes and whether any were
/// thrown away.
pub fn read_capped<R: Read>(mut pipe: R, max: usize) -> (Vec<u8>, bool) {
    let mut kept: Vec<u8> = Vec::new();
    let mut cut = false;
    let mut buf = [0u8; 4096];
    loop {
        match pipe.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let room = max.saturating_sub(kept.len());
                if n > room {
                    cut = true;
                }
                kept.extend_from_slice(&buf[..n.min(room)]);
            }
        }
    }
    (kept, cut)
}

fn drain(pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<(Vec<u8>, bool)> {
    std::thread::spawn(move || read_capped(pipe, MAX_OUTPUT_BYTES))
}

/// Runs `program` with the word of the command, `--lang`, and the arguments, and waits for it for at most `limit`. Nothing is read from the keyboard. The program is stopped when
/// the time is over.
pub fn run_blocking(
    program: &Path,
    command: Command,
    lang: Lang,
    args: &[String],
    limit: Duration,
) -> Outcome {
    let mut process = Process::new(program);
    process
        .arg(command.name())
        .arg("--lang")
        .arg(lang.code())
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // no console window opens for the command
        process.creation_flags(0x0800_0000);
    }
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(e) => {
            return Outcome {
                code: None,
                text: String::new(),
                timed_out: false,
                truncated: false,
                failed_to_start: Some(format!("{}: {e}", program.display())),
            }
        }
    };
    let out = child.stdout.take().map(drain);
    let err = child.stderr.take().map(drain);
    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if start.elapsed() >= limit {
            timed_out = true;
            let _ = child.kill();
            break child.wait().ok();
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => break None,
        }
    };
    let mut text = String::new();
    let mut truncated = false;
    for handle in [out, err].into_iter().flatten() {
        if let Ok((bytes, cut)) = handle.join() {
            truncated |= cut;
            text.push_str(&String::from_utf8_lossy(&bytes));
        }
    }
    Outcome {
        code: if timed_out {
            None
        } else {
            status.and_then(|s| s.code())
        },
        text,
        timed_out,
        truncated,
        failed_to_start: None,
    }
}

/// A panel of a command: the arguments typed, a run that may be going on and the last outcome.
#[derive(Debug)]
pub struct CmdPanel {
    pub command: Command,
    /// The arguments as they are typed.
    pub args: String,
    running: Option<Receiver<Outcome>>,
    /// What the last run came to.
    pub last: Option<Outcome>,
    /// Why the arguments were refused, when they were.
    pub refused: Option<ArgError>,
}

impl CmdPanel {
    pub fn new(command: Command) -> CmdPanel {
        CmdPanel {
            command,
            args: String::new(),
            running: None,
            last: None,
            refused: None,
        }
    }

    /// Whether a run is going on.
    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// Starts the run of the command with the arguments typed, in a thread of its own, so the window goes on drawing. Nothing starts while a run is going on or when the arguments are
    /// refused.
    pub fn start(&mut self, program: PathBuf, lang: Lang, limit: Duration) -> bool {
        if self.running.is_some() {
            return false;
        }
        let args = match split_args(&self.args) {
            Ok(args) => args,
            Err(e) => {
                self.refused = Some(e);
                return false;
            }
        };
        self.refused = None;
        let command = self.command;
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(run_blocking(&program, command, lang, &args, limit));
        });
        self.running = Some(rx);
        true
    }

    /// Takes the outcome of the run when it has finished; gives whether the panel changed.
    pub fn poll(&mut self) -> bool {
        let Some(rx) = self.running.as_ref() else {
            return false;
        };
        match rx.try_recv() {
            Ok(outcome) => {
                self.last = Some(outcome);
                self.running = None;
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.running = None;
                true
            }
        }
    }
}

/// The eleven panels.
#[derive(Debug)]
pub struct CommandBox {
    pub panels: Vec<CmdPanel>,
}

impl Default for CommandBox {
    fn default() -> Self {
        CommandBox {
            panels: Command::ALL.into_iter().map(CmdPanel::new).collect(),
        }
    }
}

impl CommandBox {
    /// The panel of a command.
    pub fn panel_mut(&mut self, command: Command) -> Option<&mut CmdPanel> {
        self.panels.iter_mut().find(|p| p.command == command)
    }

    /// Whether any run is going on (the window keeps asking for frames while one is).
    pub fn any_running(&self) -> bool {
        self.panels.iter().any(CmdPanel::is_running)
    }

    /// Takes the finished runs; gives whether any panel changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        for p in &mut self.panels {
            changed |= p.poll();
        }
        changed
    }
}
