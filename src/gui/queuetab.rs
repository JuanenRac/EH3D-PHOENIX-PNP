// EH3D-PHOENIX-PNP - the queue of jobs in the window, without the window.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! What the Queue tool does besides drawing: the file of the queue is read and shown as a list of jobs, and the buttons add a job, check the plans, confirm the plan of a job, skip, reset,
//! revoke or take out a job and run the confirmed jobs. Every change is made by the command of the same name (`queue-add`, `queue-status`, `queue-confirm`, `queue-edit`, `queue-run`) run
//! as the program that sits beside the window, so that the window and the command line plan, confirm and run in exactly one way, with the same identity of the plan and the same lock.
//!
//! A run is only ever on the simulated machine. It does not start with the first press of the button: the first press asks (when `queue_gui_ask_run` is on, the default) and only the
//! second one, with the words of what will happen on screen, starts it. The run goes on in the background, the window reads its end, and a button stops it.

use super::texts::{tr, Text};
use crate::error::Lang;
use crate::queue::{self, Entry, Kind, Queue, State};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

/// The most bytes of a queue file that the window reads.
pub const MAX_QUEUE_BYTES: u64 = 1_048_576;

/// A job of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub state: State,
    pub done: u32,
    pub boards: u32,
    pub confirmed: bool,
    pub side: String,
    pub detail: String,
}

impl Row {
    fn of(e: &Entry) -> Row {
        Row {
            name: e.name.clone(),
            state: e.state,
            done: e.done,
            boards: e.boards,
            confirmed: !e.confirmed.is_empty(),
            side: e.side.clone(),
            detail: e.detail.clone(),
        }
    }
}

/// The state of the Queue tool.
#[derive(Debug, Clone)]
pub struct QueueTab {
    /// The queue file, and the file of settings that `queue-status` and `queue-run` read (`queue_confirm`, `queue_stop_on_failure`); empty for the defaults.
    pub file: String,
    pub settings: String,
    /// The form of a new job.
    pub name: String,
    pub source: String,
    pub feeders: String,
    pub packages: String,
    pub catalog: String,
    pub job_config: String,
    pub side: String,
    pub boards: u32,
    pub panel: bool,
    /// The job that the buttons act on.
    pub selected: String,
    /// The jobs of the file, as it was read last.
    pub rows: Vec<Row>,
    /// The first press of Run was made and the second one starts the run.
    pub run_armed: bool,
    /// The program that carries the commands out (`phoenix-pnp`); the one beside the window unless a test says otherwise.
    pub program: Option<PathBuf>,
    /// What the last command printed.
    pub output: String,
    /// What was done and what went wrong, in the language of the window.
    pub notice: String,
    pub error: String,
    child: Option<Arc<Mutex<Child>>>,
    run_log: Option<PathBuf>,
}

/// The word for what became of a job, in the language of the window.
pub fn state_text(state: State, lang: Lang) -> &'static str {
    tr(
        match state {
            State::Pending => Text::QueueStatePending,
            State::Running => Text::QueueStateRunning,
            State::Done => Text::QueueStateDone,
            State::Failed => Text::QueueStateFailed,
            State::Skipped => Text::QueueStateSkipped,
        },
        lang,
    )
}

impl Row {
    /// The line of the list for the job: the name, what became of it, the boards made and whether its plan is confirmed.
    pub fn line(&self, lang: Lang) -> String {
        let mut line = format!(
            "{} - {} - {}/{} {} - {}",
            self.name,
            state_text(self.state, lang),
            self.done,
            self.boards,
            tr(Text::QueueBoardsDone, lang),
            tr(
                if self.confirmed {
                    Text::QueueConfirmed
                } else {
                    Text::QueueNotConfirmed
                },
                lang
            )
        );
        if !self.side.is_empty() {
            line.push_str(&format!(" - {}", self.side));
        }
        if !self.detail.is_empty() {
            line.push_str(&format!(" - {}", self.detail));
        }
        line
    }
}

/// The program of the commands that is in the folder of the window.
pub fn sibling_program() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let name = format!("phoenix-pnp{}", std::env::consts::EXE_SUFFIX);
    let path = exe.parent()?.join(name);
    path.is_file().then_some(path)
}

fn command(program: &Path, args: &[String]) -> Command {
    let mut c = Command::new(program);
    c.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // no console window flashes up for the command
        c.creation_flags(0x0800_0000);
    }
    c
}

impl Default for QueueTab {
    fn default() -> Self {
        QueueTab::new()
    }
}

impl QueueTab {
    pub fn new() -> QueueTab {
        QueueTab {
            file: String::new(),
            settings: String::new(),
            name: String::new(),
            source: String::new(),
            feeders: String::new(),
            packages: String::new(),
            catalog: String::new(),
            job_config: String::new(),
            side: String::new(),
            boards: 1,
            panel: false,
            selected: String::new(),
            rows: Vec::new(),
            run_armed: false,
            program: sibling_program(),
            output: String::new(),
            notice: String::new(),
            error: String::new(),
            child: None,
            run_log: None,
        }
    }

    /// Whether a run is going on.
    pub fn running(&self) -> bool {
        self.child.is_some()
    }

    fn clear_messages(&mut self) {
        self.notice.clear();
        self.error.clear();
    }

    fn lang_args(lang: Lang) -> Vec<String> {
        vec!["--lang".to_string(), lang.code().to_string()]
    }

    fn settings_args(&self) -> Vec<String> {
        if self.settings.trim().is_empty() {
            Vec::new()
        } else {
            vec!["--config".to_string(), self.settings.trim().to_string()]
        }
    }

    /// The queue file, or why there is none.
    fn file_path(&mut self, lang: Lang) -> Option<String> {
        let f = self.file.trim().to_string();
        if f.is_empty() {
            self.error = tr(Text::QueueNeedFile, lang).to_string();
            None
        } else {
            Some(f)
        }
    }

    fn program_path(&mut self, lang: Lang) -> Option<PathBuf> {
        match &self.program {
            Some(p) => Some(p.clone()),
            None => {
                self.error = tr(Text::QueueNoProgram, lang).to_string();
                None
            }
        }
    }

    fn selected_name(&mut self, lang: Lang) -> Option<String> {
        if self.rows.iter().any(|r| r.name == self.selected) {
            Some(self.selected.clone())
        } else {
            self.error = tr(Text::QueueNeedSelected, lang).to_string();
            None
        }
    }

    /// Reads the queue file again into the list. A file that is not there is an empty list; one that is not valid is told, with its code.
    pub fn refresh(&mut self, lang: Lang) {
        let path = self.file.trim().to_string();
        self.rows.clear();
        if path.is_empty() {
            return;
        }
        match queue::read_file(&path, MAX_QUEUE_BYTES) {
            Ok(q) => {
                self.rows = q.entries().iter().map(Row::of).collect();
                if !self.rows.iter().any(|r| r.name == self.selected) {
                    self.selected = self
                        .rows
                        .first()
                        .map(|r| r.name.clone())
                        .unwrap_or_default();
                }
            }
            Err(e) => self.error = e.localized(lang),
        }
    }

    /// Runs a command to its end and gives whether it succeeded with what it printed.
    fn call(&mut self, args: &[String], lang: Lang) -> Option<(bool, String)> {
        let program = self.program_path(lang)?;
        match command(&program, args).stdin(Stdio::null()).output() {
            Ok(o) => {
                let mut text = String::from_utf8_lossy(&o.stdout).into_owned();
                text.push_str(&String::from_utf8_lossy(&o.stderr));
                Some((o.status.success(), text))
            }
            Err(e) => {
                self.error = format!("{}: {e}", program.display());
                None
            }
        }
    }

    /// Runs a command that changes the queue, reads the file again and says what became of it.
    fn change(&mut self, args: Vec<String>, lang: Lang) -> bool {
        let Some((ok, text)) = self.call(&args, lang) else {
            return false;
        };
        self.output = text.clone();
        self.refresh(lang);
        if ok {
            self.notice = text.lines().next().unwrap_or_default().to_string();
        } else {
            self.error = text.trim().to_string();
        }
        ok
    }

    /// Adds the job of the form to the queue. The form is checked here first, so that what is wrong is told in the language of the window.
    pub fn add(&mut self, relative: bool, lang: Lang) -> bool {
        self.clear_messages();
        let Some(file) = self.file_path(lang) else {
            return false;
        };
        let kind = if self.panel { Kind::Panel } else { Kind::Parts };
        let probe = Entry::new(
            self.name.trim(),
            kind,
            self.source.trim(),
            self.feeders.trim(),
            self.boards,
        );
        if let Err(e) = Queue::new().add(probe) {
            self.error = e.localized(lang);
            return false;
        }
        let mut args = vec![
            "queue-add".to_string(),
            file,
            self.name.trim().to_string(),
            self.source.trim().to_string(),
            self.feeders.trim().to_string(),
            "--boards".to_string(),
            self.boards.to_string(),
        ];
        if self.panel {
            args.push("--panel".to_string());
        }
        if relative {
            args.push("--relative".to_string());
        }
        for (flag, v) in [
            ("--side", &self.side),
            ("--config", &self.job_config),
            ("--packages", &self.packages),
            ("--catalog", &self.catalog),
        ] {
            if !v.trim().is_empty() {
                args.push(flag.to_string());
                args.push(v.trim().to_string());
            }
        }
        args.extend(Self::lang_args(lang));
        let added = self.change(args, lang);
        if added {
            self.selected = self.name.trim().to_string();
            self.name.clear();
        }
        added
    }

    /// Plans the jobs that are still to be made and says which could be run now and which could not, and why (nothing is changed).
    pub fn check(&mut self, lang: Lang) {
        self.clear_messages();
        let (Some(file), Some(_)) = (self.file_path(lang), self.program_path(lang)) else {
            return;
        };
        let mut args = vec!["queue-status".to_string(), file, "--check".to_string()];
        args.extend(self.settings_args());
        args.extend(Self::lang_args(lang));
        if let Some((ok, text)) = self.call(&args, lang) {
            self.output = text;
            self.notice = tr(
                if ok {
                    Text::QueueCheckedOk
                } else {
                    Text::QueueCheckedBlocked
                },
                lang,
            )
            .to_string();
        }
    }

    /// Records that the person looked at the plan of the selected job as it is now and wants it run.
    pub fn confirm(&mut self, lang: Lang) -> bool {
        self.clear_messages();
        self.run_armed = false;
        let (Some(file), Some(name)) = (self.file_path(lang), self.selected_name(lang)) else {
            return false;
        };
        let mut args = vec!["queue-confirm".to_string(), file, name];
        args.extend(Self::lang_args(lang));
        self.change(args, lang)
    }

    /// Skips, resets, revokes the confirmation of or takes out the selected job (`action` is one of those four words of `queue-edit`).
    pub fn edit(&mut self, action: &str, lang: Lang) -> bool {
        self.clear_messages();
        self.run_armed = false;
        if !matches!(action, "skip" | "reset" | "revoke" | "remove") {
            self.error = format!("'{action}' is not skip, reset, revoke or remove");
            return false;
        }
        let (Some(file), Some(name)) = (self.file_path(lang), self.selected_name(lang)) else {
            return false;
        };
        let mut args = vec!["queue-edit".to_string(), file, name, action.to_string()];
        args.extend(Self::lang_args(lang));
        self.change(args, lang)
    }

    /// The press of the Run button. With `ask` (the setting `queue_gui_ask_run`) the first press only asks and the run starts with the second; without it the run starts at once.
    /// Gives whether the run started.
    pub fn press_run(&mut self, ask: bool, lang: Lang) -> bool {
        self.clear_messages();
        if self.child.is_some() {
            self.error = tr(Text::QueueAlreadyRunning, lang).to_string();
            return false;
        }
        if self.file_path(lang).is_none() || self.program_path(lang).is_none() {
            self.run_armed = false;
            return false;
        }
        self.refresh(lang);
        if !self
            .rows
            .iter()
            .any(|r| matches!(r.state, State::Pending | State::Running))
        {
            self.run_armed = false;
            self.error = tr(Text::QueueNothingToRun, lang).to_string();
            return false;
        }
        if ask && !self.run_armed {
            self.run_armed = true;
            self.notice = tr(Text::QueueAskRun, lang).to_string();
            return false;
        }
        self.run_armed = false;
        self.start_run(lang)
    }

    /// Takes the first press back.
    pub fn cancel_run(&mut self) {
        self.run_armed = false;
        self.notice.clear();
    }

    fn start_run(&mut self, lang: Lang) -> bool {
        let (Some(file), Some(program)) = (self.file_path(lang), self.program_path(lang)) else {
            return false;
        };
        let mut args = vec!["queue-run".to_string(), file];
        args.extend(self.settings_args());
        args.extend(Self::lang_args(lang));
        let log = std::env::temp_dir().join(format!(
            "phoenix_pnp_queue_run_{}_{}.txt",
            std::process::id(),
            self.rows.len()
        ));
        let started = std::fs::File::create(&log)
            .and_then(|out| out.try_clone().map(|err| (out, err)))
            .and_then(|(out, err)| {
                command(&program, &args)
                    .stdin(Stdio::null())
                    .stdout(Stdio::from(out))
                    .stderr(Stdio::from(err))
                    .spawn()
            });
        match started {
            Ok(child) => {
                self.child = Some(Arc::new(Mutex::new(child)));
                self.run_log = Some(log);
                self.output.clear();
                self.notice = tr(Text::QueueRunStarted, lang).to_string();
                true
            }
            Err(e) => {
                let _ = std::fs::remove_file(&log);
                self.error = format!("{}: {e}", program.display());
                false
            }
        }
    }

    fn finish_run(&mut self) -> String {
        self.child = None;
        let text = self
            .run_log
            .take()
            .map(|p| {
                let t = std::fs::read(&p)
                    .map(|b| String::from_utf8_lossy(&b).into_owned())
                    .unwrap_or_default();
                let _ = std::fs::remove_file(&p);
                t
            })
            .unwrap_or_default();
        self.output = text.clone();
        text
    }

    /// Looks whether the run ended; when it did, reads what it printed and the file again. Gives whether a run is still going on.
    pub fn poll(&mut self, lang: Lang) -> bool {
        let Some(child) = self.child.clone() else {
            return false;
        };
        let status = match child.lock() {
            Ok(mut c) => c.try_wait(),
            Err(_) => return true,
        };
        match status {
            Ok(None) => true,
            Ok(Some(status)) => {
                let text = self.finish_run();
                self.refresh(lang);
                if status.success() {
                    self.notice = tr(Text::QueueRunFinished, lang).to_string();
                } else {
                    self.error = text
                        .lines()
                        .rev()
                        .find(|l| !l.trim().is_empty())
                        .unwrap_or_default()
                        .to_string();
                    if self.error.is_empty() {
                        self.error = tr(Text::QueueRunFailed, lang).to_string();
                    }
                }
                false
            }
            Err(e) => {
                self.finish_run();
                self.error = e.to_string();
                false
            }
        }
    }

    /// Stops the run. The job under way stays `running` in the file, to be run again from where it was; the lock of the run is taken away.
    pub fn stop(&mut self, lang: Lang) {
        self.clear_messages();
        if let Some(child) = self.child.clone() {
            if let Ok(mut c) = child.lock() {
                let _ = c.kill();
                let _ = c.wait();
            }
            self.finish_run();
            let _ = std::fs::remove_file(format!("{}.lock", self.file.trim()));
            self.refresh(lang);
            self.notice = tr(Text::QueueRunStopped, lang).to_string();
        }
    }
}
