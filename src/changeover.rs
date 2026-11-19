// EH3D-PHOENIX-PNP - what has to be changed on the feeders between one job of a queue and the next.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A day's work is a queue of jobs. The jobs that share a table of feeders can follow each other with no change; the ones that do not need a person to put other reels in the places of the
//! feeders. This takes the jobs that are still to be made, in the order of the queue, compares the table of feeders of each job with the one of the job before it and says what has to be
//! done between them: the feeders that hold another kind of part (a reel to change), the feeders that are switched on for the next job and were not for the one before (a reel to put in),
//! the feeders that the next job does not use (a reel that may stay) and how many stay as they are. Nothing is written to the queue and nothing is run.

use crate::feeding::FeederSpec;
use crate::parts::csv_field;
use std::collections::BTreeMap;

/// What to do with one feeder between two jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// The feeder holds another kind of part: the reel is changed.
    Change,
    /// The feeder was off or not there and the next job has it on: a reel is put in.
    Load,
    /// The feeder is on for the job before and not for the next one: its reel may stay.
    Free,
}

impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Action::Change => "change",
            Action::Load => "load",
            Action::Free => "free",
        }
    }
}

/// One feeder that has to be looked at between two jobs.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub id: String,
    pub slot: String,
    pub action: Action,
    /// What the feeder held in the first job and what it holds in the next, as `value / package` (empty for a feeder that was not on).
    pub from: String,
    pub to: String,
}

/// What happens between a job and the next.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub from_job: String,
    pub to_job: String,
    pub changes: Vec<Change>,
    /// The feeders that stay as they are.
    pub kept: usize,
}

impl Step {
    /// The feeders where a reel is changed or put in: the work of the person between the jobs.
    pub fn work(&self) -> usize {
        self.changes
            .iter()
            .filter(|c| c.action != Action::Free)
            .count()
    }
}

fn kind_of(f: &FeederSpec) -> String {
    format!("{} / {}", f.base.value.trim(), f.base.package.trim())
}

fn on(feeders: &[FeederSpec]) -> BTreeMap<&str, &FeederSpec> {
    feeders
        .iter()
        .filter(|f| f.enabled)
        .map(|f| (f.base.id.as_str(), f))
        .collect()
}

/// What has to be done on the feeders to go from the table `a` to the table `b`: the feeders are found by their id and only the ones that are switched on count.
pub fn between(a: &[FeederSpec], b: &[FeederSpec]) -> (Vec<Change>, usize) {
    let (before, after) = (on(a), on(b));
    let mut changes = Vec::new();
    let mut kept = 0usize;
    for (id, f) in &after {
        let slot = f.slot.clone().unwrap_or_default();
        match before.get(id) {
            Some(g) if g.base.key() == f.base.key() => kept += 1,
            Some(g) => changes.push(Change {
                id: (*id).to_string(),
                slot,
                action: Action::Change,
                from: kind_of(g),
                to: kind_of(f),
            }),
            None => changes.push(Change {
                id: (*id).to_string(),
                slot,
                action: Action::Load,
                from: String::new(),
                to: kind_of(f),
            }),
        }
    }
    for (id, g) in &before {
        if !after.contains_key(id) {
            changes.push(Change {
                id: (*id).to_string(),
                slot: g.slot.clone().unwrap_or_default(),
                action: Action::Free,
                from: kind_of(g),
                to: String::new(),
            });
        }
    }
    changes.sort_by(|x, y| x.id.cmp(&y.id));
    (changes, kept)
}

/// The steps between the jobs in the order they are given; a job is `(name, table of feeders)`.
pub fn steps(jobs: &[(String, Vec<FeederSpec>)]) -> Vec<Step> {
    jobs.windows(2)
        .map(|w| {
            let (changes, kept) = between(&w[0].1, &w[1].1);
            Step {
                from_job: w[0].0.clone(),
                to_job: w[1].0.clone(),
                changes,
                kept,
            }
        })
        .collect()
}

/// The steps as text: a line for each step, then a line for each feeder to look at, then the line that sums up.
pub fn text(steps: &[Step]) -> String {
    let mut out = String::new();
    for s in steps {
        if s.changes.is_empty() {
            out.push_str(&format!(
                "{} -> {}: nothing to change, {} feeders stay as they are\n",
                s.from_job, s.to_job, s.kept
            ));
            continue;
        }
        out.push_str(&format!(
            "{} -> {}: {} feeders to change or load, {} stay as they are\n",
            s.from_job,
            s.to_job,
            s.work(),
            s.kept
        ));
        for c in &s.changes {
            let place = if c.slot.is_empty() {
                String::new()
            } else {
                format!(" (slot {})", c.slot)
            };
            match c.action {
                Action::Change => out.push_str(&format!(
                    "  {}{place}: change {} for {}\n",
                    c.id, c.from, c.to
                )),
                Action::Load => out.push_str(&format!("  {}{place}: load {}\n", c.id, c.to)),
                Action::Free => out.push_str(&format!(
                    "  {}{place}: {} is not used and may stay\n",
                    c.id, c.from
                )),
            }
        }
    }
    let work: usize = steps.iter().map(Step::work).sum();
    let stops = steps.iter().filter(|s| s.work() > 0).count();
    if steps.is_empty() {
        out.push_str("nothing to compare: fewer than two jobs are still to be made\n");
    } else if work == 0 {
        out.push_str(&format!(
            "ok: the {} jobs follow each other with no reel to change\n",
            steps.len() + 1
        ));
    } else {
        out.push_str(&format!(
            "{work} reels to change or load in {stops} of the {} changes between jobs\n",
            steps.len()
        ));
    }
    out
}

/// The steps as a CSV: `from_job,to_job,feeder,slot,action,from,to`.
pub fn csv(steps: &[Step]) -> String {
    let mut out = String::from("from_job,to_job,feeder,slot,action,from,to\n");
    for s in steps {
        for c in &s.changes {
            out.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                csv_field(&s.from_job),
                csv_field(&s.to_job),
                csv_field(&c.id),
                csv_field(&c.slot),
                c.action.name(),
                csv_field(&c.from),
                csv_field(&c.to)
            ));
        }
    }
    out
}

/// What the jobs of a queue file ask of the feeders between them, as the text the command prints, the table to write and the files that were read.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueChanges {
    pub text: String,
    pub csv: String,
    /// The reels to change or load, all together.
    pub work: usize,
    /// The queue file and the tables of feeders that were read: what an output must not be written over.
    pub inputs: Vec<String>,
}

/// The changes of the feeders between the jobs of the queue file `path` that are still to be made, in the order of the queue. `read` reads a text file of at most the given bytes. This is what
/// `queue-changeover` and the Queue changeover tool of the window both run.
pub fn from_queue(
    path: &str,
    cfg: &crate::config::MachineConfig,
    lang: crate::error::Lang,
    read: &dyn Fn(&str, u64) -> Result<String, String>,
) -> Result<QueueChanges, String> {
    use crate::queue;
    use crate::queuestock::{queue_dir, MAX_QUEUE_BYTES};
    if !std::path::Path::new(path).is_file() {
        return Err(format!("{path}: there is no such queue file"));
    }
    let q = queue::read_file(path, MAX_QUEUE_BYTES).map_err(|e| e.localized(lang))?;
    let dir = queue_dir(path);
    let mut jobs: Vec<(String, Vec<FeederSpec>)> = Vec::new();
    let mut inputs = vec![path.to_string()];
    let mut tables: BTreeMap<String, Vec<FeederSpec>> = BTreeMap::new();
    for e in q.to_do() {
        if e.boards.saturating_sub(e.done) == 0 {
            continue;
        }
        let table = queue::resolve(&e.feeders, &dir);
        if !tables.contains_key(&table) {
            let feeders =
                crate::feeding::from_csv_with(&read(&table, cfg.max_file_bytes)?, cfg.tray_fill)
                    .map_err(|er| er.localized(lang))?;
            tables.insert(table.clone(), feeders);
            inputs.push(table.clone());
        }
        jobs.push((e.name.clone(), tables[&table].clone()));
    }
    let found = steps(&jobs);
    Ok(QueueChanges {
        text: text(&found),
        csv: csv(&found),
        work: found.iter().map(Step::work).sum(),
        inputs,
    })
}
