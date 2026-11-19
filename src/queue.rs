// EH3D-PHOENIX-PNP - a queue of jobs: several boards or panels one after the other.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A run of many boards is a list of jobs, each a table of parts (or a panel) with its feeders, its settings and how many boards to make. The queue keeps that list and what became of each
//! entry in a small CSV that survives the program stopping: it is written to a temporary file and moved into place, the file it replaces is kept as `PATH.prev`, and the last line holds a
//! checksum of everything above it, so a file that was cut or changed by hand is refused instead of being believed.
//!
//! Nothing here runs a job and nothing moves a machine. What the queue adds is the gate before a job: an entry may be run only when the person confirmed it, and the confirmation is for the
//! plan as it was when it was given (its identity, a hash of the plan): when the files or the settings change, the plan changes and the confirmation is void. `queue_confirm = off` lifts
//! the gate, for the simulated machine; with a machine that moves it should stay on.
//!
//! The file has the header `name,kind,source,feeders,config,packages,catalog,side,boards,done,state,confirmed,detail` and a row for each entry; lines that start with `#` are notes. Every
//! error carries the code `PHOENIX-PNP-060`.

use crate::error::{Code, PnpError};
use crate::parts::{csv_field, read_records_numbered};
use std::path::Path;

/// The most entries of a queue.
pub const MAX_ENTRIES: usize = 500;
/// The most boards of an entry.
pub const MAX_BOARDS: u32 = 1_000_000;
const HEADER: &str =
    "name,kind,source,feeders,config,packages,catalog,side,boards,done,state,confirmed,detail";
const SUM: &str = "# checksum = ";

fn bad(message: impl Into<String>) -> PnpError {
    PnpError::new(Code::QueueBad, message)
}

fn fnv1a(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// What became of an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    /// Not started.
    #[default]
    Pending,
    /// Started and not finished: the program stopped in the middle, or a board is under way.
    Running,
    /// All its boards were made.
    Done,
    /// A board failed; the reason is in the detail.
    Failed,
    /// Left out by the person.
    Skipped,
}

impl State {
    pub const ALL: [State; 5] = [
        State::Pending,
        State::Running,
        State::Done,
        State::Failed,
        State::Skipped,
    ];

    pub fn name(self) -> &'static str {
        match self {
            State::Pending => "pending",
            State::Running => "running",
            State::Done => "done",
            State::Failed => "failed",
            State::Skipped => "skipped",
        }
    }

    pub fn parse(text: &str) -> Option<State> {
        let t = text.trim().to_ascii_lowercase();
        State::ALL.into_iter().find(|s| s.name() == t)
    }
}

/// Whether the source of an entry is a table of parts or a panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Parts,
    Panel,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Parts => "parts",
            Kind::Panel => "panel",
        }
    }

    pub fn parse(text: &str) -> Option<Kind> {
        match text.trim().to_ascii_lowercase().as_str() {
            "parts" => Some(Kind::Parts),
            "panel" => Some(Kind::Panel),
            _ => None,
        }
    }
}

/// `path` as a queue file in `dir` keeps it when the paths are kept relative (`queue_relative_paths`): the path from `dir`, with `/` between the names, when the file is in `dir` or below it;
/// any other path (or one that is not absolute) is kept as it is.
pub fn relative_to(path: &str, dir: &Path) -> String {
    let p = Path::new(path);
    if !p.is_absolute() {
        return path.to_string();
    }
    match p.strip_prefix(dir) {
        Ok(rest) if !rest.as_os_str().is_empty() => rest
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
        _ => path.to_string(),
    }
}

/// `path` as it is found from where the queue file is: an empty path stays empty, an absolute one stays as it is and a relative one is taken from `dir`.
pub fn resolve(path: &str, dir: &Path) -> String {
    if path.trim().is_empty() || Path::new(path).is_absolute() {
        return path.to_string();
    }
    dir.join(path).to_string_lossy().into_owned()
}

/// One job of the queue.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Entry {
    /// Letters, digits, `_`, `-` and `.`, 1 to 64 characters; it names the entry in the commands and its file of state.
    pub name: String,
    pub kind: Kind,
    /// The table of parts, or the panel.
    pub source: String,
    pub feeders: String,
    /// The files of settings and tables that go with the job (empty for none).
    pub config: String,
    pub packages: String,
    pub catalog: String,
    /// `top`, `bottom` or empty for the default of the program.
    pub side: String,
    /// How many boards to make, and how many are made.
    pub boards: u32,
    pub done: u32,
    pub state: State,
    /// The identity of the plan that the person confirmed (empty: not confirmed).
    pub confirmed: String,
    /// What happened last, in a line.
    pub detail: String,
}

/// Whether `name` can name an entry.
pub fn name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        && name != "."
        && name != ".."
}

fn text_ok(t: &str) -> bool {
    t.chars().count() <= 400 && !t.chars().any(char::is_control)
}

impl Entry {
    /// A new entry for the boards of a job: not started, not confirmed.
    pub fn new(name: &str, kind: Kind, source: &str, feeders: &str, boards: u32) -> Entry {
        Entry {
            name: name.to_string(),
            kind,
            source: source.to_string(),
            feeders: feeders.to_string(),
            boards,
            ..Entry::default()
        }
    }

    fn check(&self) -> Result<(), PnpError> {
        if !name_ok(&self.name) {
            return Err(bad(format!(
                "the name '{}' is empty, too long or has a character other than letters, digits, _ - and .",
                self.name
            )));
        }
        for (what, v) in [
            ("source", &self.source),
            ("feeders", &self.feeders),
            ("config", &self.config),
            ("packages", &self.packages),
            ("catalog", &self.catalog),
            ("detail", &self.detail),
        ] {
            if !text_ok(v) {
                return Err(bad(format!(
                    "{}: the {what} is too long or has a control character",
                    self.name
                )));
            }
        }
        if self.source.trim().is_empty() || self.feeders.trim().is_empty() {
            return Err(bad(format!(
                "{}: the source and the feeders are needed",
                self.name
            )));
        }
        if !matches!(self.side.as_str(), "" | "top" | "bottom") {
            return Err(bad(format!(
                "{}: the side '{}' is not top or bottom",
                self.name, self.side
            )));
        }
        if self.boards == 0 || self.boards > MAX_BOARDS {
            return Err(bad(format!(
                "{}: the boards are {}, not from 1 to {MAX_BOARDS}",
                self.name, self.boards
            )));
        }
        if self.done > self.boards {
            return Err(bad(format!(
                "{}: {} boards done of {}",
                self.name, self.done, self.boards
            )));
        }
        if self.confirmed.len() > 64 || !self.confirmed.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(bad(format!(
                "{}: the confirmation is not the identity of a plan",
                self.name
            )));
        }
        Ok(())
    }

    /// The arguments of the commands that plan and run the job: the files and the side, in the words of the command line.
    pub fn arguments(&self) -> Vec<String> {
        let mut a = Vec::new();
        for (flag, v) in [
            ("--config", &self.config),
            ("--packages", &self.packages),
            ("--catalog", &self.catalog),
            ("--side", &self.side),
        ] {
            if !v.trim().is_empty() {
                a.push(flag.to_string());
                a.push(v.clone());
            }
        }
        if self.kind == Kind::Panel {
            a.push("--panel".to_string());
            a.push(self.source.clone());
        } else {
            a.push(self.source.clone());
        }
        a.push(self.feeders.clone());
        a
    }

    /// The arguments of the commands as `arguments` gives them, with every path that is relative taken from `dir` (the folder of the queue file), so that a queue that keeps relative paths
    /// can be run from any folder.
    pub fn arguments_in(&self, dir: &Path) -> Vec<String> {
        let mut e = self.clone();
        for v in [
            &mut e.source,
            &mut e.feeders,
            &mut e.config,
            &mut e.packages,
            &mut e.catalog,
        ] {
            *v = resolve(v, dir);
        }
        e.arguments()
    }
}

/// The jobs of a run, in the order they are to be made.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Queue {
    entries: Vec<Entry>,
}

impl Queue {
    pub fn new() -> Queue {
        Queue::default()
    }

    /// Reads a queue. The line `# checksum = ...`, when there is one, must be the checksum of everything above it; a file with no such line is taken as written by hand.
    pub fn parse(text: &str) -> Result<Queue, PnpError> {
        let text = text.trim_start_matches('\u{feff}');
        let text = text.replace("\r\n", "\n");
        let mut body = String::new();
        let mut sum: Option<String> = None;
        let mut checked = String::new();
        for line in text.split_inclusive('\n') {
            if let Some(rest) = line.trim_end().strip_prefix(SUM.trim_end()) {
                let value = rest.trim_start_matches([' ', '=']).trim();
                if sum.is_some() {
                    return Err(bad("the checksum is given twice"));
                }
                sum = Some(value.to_string());
                // what the checksum covers is everything before its line
                checked = body.clone();
            } else if sum.is_some() && !line.trim().is_empty() {
                return Err(bad("there is text after the checksum"));
            } else if sum.is_none() {
                body.push_str(line);
            }
        }
        if let Some(value) = &sum {
            let want = format!("{:016x}", fnv1a(&checked));
            if *value != want {
                return Err(bad(
                    "the checksum does not match: the file was cut or changed by hand (the copy kept before the last write is PATH.prev)",
                ));
            }
        }
        let rows: Vec<&str> = body
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
            .collect();
        if rows.is_empty() {
            return Err(bad("the file is empty"));
        }
        let records = read_records_numbered(&rows.join("\n"), ',')
            .map_err(|(line, e)| bad(format!("row {line}: {e}")))?;
        let mut it = records.into_iter();
        let head: Vec<String> = it
            .next()
            .map(|(_, r)| r.iter().map(|h| h.to_ascii_lowercase()).collect())
            .unwrap_or_default();
        if head.join(",") != HEADER {
            return Err(bad(format!("the first row must be exactly: {HEADER}")));
        }
        let mut queue = Queue::new();
        for (row, f) in it {
            if f.len() != 13 {
                return Err(bad(format!("row {row}: {} fields, not 13", f.len())));
            }
            let number = |cell: &str, what: &str| -> Result<u32, PnpError> {
                cell.trim()
                    .parse::<u32>()
                    .map_err(|_| bad(format!("row {row}: {what} '{cell}' is not a whole number")))
            };
            let entry = Entry {
                name: f[0].clone(),
                kind: Kind::parse(&f[1]).ok_or_else(|| {
                    bad(format!(
                        "row {row}: the kind '{}' is not parts or panel",
                        f[1]
                    ))
                })?,
                source: f[2].clone(),
                feeders: f[3].clone(),
                config: f[4].clone(),
                packages: f[5].clone(),
                catalog: f[6].clone(),
                side: f[7].to_ascii_lowercase(),
                boards: number(&f[8], "the boards")?,
                done: number(&f[9], "the boards done")?,
                state: State::parse(&f[10])
                    .ok_or_else(|| bad(format!("row {row}: the state '{}' is not known", f[10])))?,
                confirmed: f[11].clone(),
                detail: f[12].clone(),
            };
            queue
                .add(entry)
                .map_err(|e| bad(format!("row {row}: {}", e.detail)))?;
        }
        Ok(queue)
    }

    /// The queue as text, with the checksum of the text as its last line; `parse` reads it back as the same queue.
    pub fn to_text(&self) -> String {
        let mut out = format!("{HEADER}\n");
        for e in &self.entries {
            let fields = [
                csv_field(&e.name),
                e.kind.name().to_string(),
                csv_field(&e.source),
                csv_field(&e.feeders),
                csv_field(&e.config),
                csv_field(&e.packages),
                csv_field(&e.catalog),
                e.side.clone(),
                e.boards.to_string(),
                e.done.to_string(),
                e.state.name().to_string(),
                e.confirmed.clone(),
                csv_field(&e.detail),
            ];
            out.push_str(&fields.join(","));
            out.push('\n');
        }
        let sum = fnv1a(&out);
        out.push_str(&format!("{SUM}{sum:016x}\n"));
        out
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }

    fn get_mut(&mut self, name: &str) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.name == name)
    }

    /// Adds an entry at the end. A name that is used, an entry that is not valid or more than `MAX_ENTRIES` is refused.
    pub fn add(&mut self, entry: Entry) -> Result<(), PnpError> {
        entry.check()?;
        if self.get(&entry.name).is_some() {
            return Err(bad(format!("the name '{}' is used twice", entry.name)));
        }
        if self.entries.len() >= MAX_ENTRIES {
            return Err(bad(format!("more than {MAX_ENTRIES} entries")));
        }
        self.entries.push(entry);
        Ok(())
    }

    /// Takes an entry out; the one that is running cannot be taken out.
    pub fn remove(&mut self, name: &str) -> Result<(), PnpError> {
        let i = self
            .entries
            .iter()
            .position(|e| e.name == name)
            .ok_or_else(|| bad(format!("there is no entry '{name}'")))?;
        if self.entries[i].state == State::Running {
            return Err(bad(format!(
                "'{name}' is running: skip it or reset it before taking it out"
            )));
        }
        self.entries.remove(i);
        Ok(())
    }

    /// Records that the person confirmed the plan `plan_hash` for the entry. An entry that is done or skipped is not confirmed.
    pub fn confirm(&mut self, name: &str, plan_hash: &str) -> Result<(), PnpError> {
        let e = self
            .get_mut(name)
            .ok_or_else(|| bad(format!("there is no entry '{name}'")))?;
        if matches!(e.state, State::Done | State::Skipped) {
            return Err(bad(format!(
                "'{name}' is {}: reset it to run it again",
                e.state.name()
            )));
        }
        if plan_hash.is_empty() || !plan_hash.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(bad("the identity of the plan is not valid"));
        }
        e.confirmed = plan_hash.to_string();
        Ok(())
    }

    /// Takes the confirmation of an entry back.
    pub fn revoke(&mut self, name: &str) -> Result<(), PnpError> {
        let e = self
            .get_mut(name)
            .ok_or_else(|| bad(format!("there is no entry '{name}'")))?;
        e.confirmed.clear();
        Ok(())
    }

    /// Leaves an entry out of the run.
    pub fn skip(&mut self, name: &str) -> Result<(), PnpError> {
        self.set_state(name, State::Skipped, "left out by the person")
    }

    /// Sets what became of an entry, with a line that says why (a line break in it is a space).
    pub fn set_state(&mut self, name: &str, state: State, detail: &str) -> Result<(), PnpError> {
        let e = self
            .get_mut(name)
            .ok_or_else(|| bad(format!("there is no entry '{name}'")))?;
        e.state = state;
        e.detail = detail
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .take(300)
            .collect();
        Ok(())
    }

    /// Counts a board of the entry as made; the entry is done when all its boards are.
    pub fn board_done(&mut self, name: &str) -> Result<(), PnpError> {
        let e = self
            .get_mut(name)
            .ok_or_else(|| bad(format!("there is no entry '{name}'")))?;
        e.done = (e.done + 1).min(e.boards);
        if e.done >= e.boards {
            e.state = State::Done;
            e.detail = format!("{} boards made", e.boards);
        } else {
            e.state = State::Running;
            e.detail = format!("{} of {} boards made", e.done, e.boards);
        }
        Ok(())
    }

    /// Puts an entry back to the start: not started, no boards done, and not confirmed (a confirmation is for one run).
    pub fn reset(&mut self, name: &str) -> Result<(), PnpError> {
        let e = self
            .get_mut(name)
            .ok_or_else(|| bad(format!("there is no entry '{name}'")))?;
        e.state = State::Pending;
        e.done = 0;
        e.confirmed.clear();
        e.detail.clear();
        Ok(())
    }

    /// Whether the entry may be run now for the plan `plan_hash`: it is not done, failed or skipped, and, when `need_confirm`, the person confirmed that very plan. The reason when not.
    pub fn allowed(&self, name: &str, plan_hash: &str, need_confirm: bool) -> Result<(), String> {
        let e = self
            .get(name)
            .ok_or_else(|| format!("there is no entry '{name}'"))?;
        match e.state {
            State::Pending | State::Running => {}
            other => {
                return Err(format!(
                    "'{name}' is {}: reset it to run it again",
                    other.name()
                ))
            }
        }
        if need_confirm {
            if e.confirmed.is_empty() {
                return Err(format!(
                    "'{name}' is not confirmed: look at the plan and confirm it first"
                ));
            }
            if e.confirmed != plan_hash {
                return Err(format!(
                    "'{name}' was confirmed for another plan (the files or the settings changed since): confirm it again"
                ));
            }
        }
        Ok(())
    }

    /// The entries that are still to be made, in order (pending or running).
    pub fn to_do(&self) -> impl Iterator<Item = &Entry> {
        self.entries
            .iter()
            .filter(|e| matches!(e.state, State::Pending | State::Running))
    }

    /// How many entries are in each state, in the order of `State::ALL`.
    pub fn counts(&self) -> [usize; 5] {
        let mut c = [0; 5];
        for e in &self.entries {
            if let Some(i) = State::ALL.iter().position(|s| *s == e.state) {
                c[i] += 1;
            }
        }
        c
    }

    /// The queue as a table for a person: a line for each entry and one that counts them.
    pub fn text(&self) -> String {
        if self.entries.is_empty() {
            return "the queue is empty\n".to_string();
        }
        let mut out = String::new();
        for e in &self.entries {
            out.push_str(&format!(
                "{}: {} {}/{} boards{}{}{}\n",
                e.name,
                e.state.name(),
                e.done,
                e.boards,
                if e.confirmed.is_empty() {
                    ", not confirmed"
                } else {
                    ", confirmed"
                },
                if e.side.is_empty() {
                    String::new()
                } else {
                    format!(", {}", e.side)
                },
                if e.detail.is_empty() {
                    String::new()
                } else {
                    format!(" - {}", e.detail)
                }
            ));
        }
        let c = self.counts();
        out.push_str(&format!(
            "{} entries: {} pending, {} running, {} done, {} failed, {} skipped\n",
            self.entries.len(),
            c[0],
            c[1],
            c[2],
            c[3],
            c[4]
        ));
        out
    }
}

/// The identity of a plan as the person saw it: the hash of the plan and the text of every file it was made from. Any change to a file, even a byte of the settings, changes it.
pub fn identity(plan_hash: &str, files: &[String]) -> String {
    let mut text = String::from(plan_hash);
    for f in files {
        text.push('\u{1f}');
        text.push_str(f);
    }
    format!("{:016x}", fnv1a(&text))
}

/// Reads the queue at `path`. A file that is not there is an empty queue; one that is there and is not valid is refused, and so is one larger than `max_bytes`.
pub fn read_file(path: &str, max_bytes: u64) -> Result<Queue, PnpError> {
    let p = Path::new(path);
    if !p.exists() {
        return Ok(Queue::new());
    }
    let meta = std::fs::metadata(p).map_err(|e| bad(format!("{path}: {e}")))?;
    if meta.len() > max_bytes {
        return Err(PnpError::new(
            Code::FileTooBig,
            format!("{path} has {} bytes, the limit is {max_bytes}", meta.len()),
        ));
    }
    let text = std::fs::read_to_string(p).map_err(|e| bad(format!("{path}: {e}")))?;
    Queue::parse(&text).map_err(|e| bad(format!("{path}: {}", e.detail)))
}

/// Writes the queue: the file it replaces is copied to `PATH.prev`, the new text is written to `PATH.tmp` and moved into place.
pub fn write_file(path: &str, queue: &Queue) -> Result<(), PnpError> {
    let p = Path::new(path);
    if p.exists() {
        std::fs::copy(p, format!("{path}.prev")).map_err(|e| bad(format!("{path}.prev: {e}")))?;
    }
    let tmp = format!("{path}.tmp");
    std::fs::write(&tmp, queue.to_text())
        .and_then(|()| std::fs::rename(&tmp, p))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            bad(format!("{path}: {e}"))
        })
}
