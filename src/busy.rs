// EH3D-PHOENIX-PNP - where the time of a simulated job goes, nozzle by nozzle and feeder by feeder, read from its journal.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! `simulate --journal` writes one row for each step (`step,at_s,took_s,action,reference,feeder,nozzle,result,status`). This reads that file back and says how busy each nozzle and each
//! feeder was (the seconds of its steps and their share of the time of the job), how many steps did not end well, and where the machine waited: the longest stretches between the end of
//! a step and the start of the next. It reads a file and changes nothing; a damaged row is an error with its line.

use crate::error::{Code, PnpError};
use crate::journal::NozzleState;
use crate::parts::{csv_field, detect_delimiter, read_records_numbered};
use std::collections::BTreeMap;

/// The most rows of a journal that are read.
pub const MAX_STEPS: usize = 1_000_000;

/// One row of the journal.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub step: usize,
    pub at_s: f64,
    pub took_s: f64,
    pub action: String,
    pub reference: String,
    pub feeder: String,
    pub nozzle: usize,
    pub result: String,
    pub status: String,
    /// What the nozzle was doing when the step was over (`idle`, `carrying`...; see `journal::NozzleState`); empty in a journal that has no such column.
    pub nozzle_state: String,
}

fn bad(line: usize, what: impl Into<String>) -> PnpError {
    PnpError::new(Code::ImportBad, format!("line {line}: {}", what.into()))
}

/// Reads the journal that `simulate --journal` writes (the columns are found by name).
pub fn read_journal(text: &str) -> Result<Vec<Step>, PnpError> {
    let text = text.trim_start_matches('\u{feff}');
    let head = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let records =
        read_records_numbered(text, detect_delimiter(head)).map_err(|(line, e)| bad(line, e))?;
    let mut rows = records.into_iter();
    let (hline, header) = rows.next().ok_or_else(|| bad(1, "the journal is empty"))?;
    let keys: Vec<String> = header
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();
    let at = |name: &str| {
        keys.iter()
            .position(|k| k == name)
            .ok_or_else(|| bad(hline, format!("the header has no column {name}")))
    };
    let (c_step, c_at, c_took, c_action) = (at("step")?, at("at_s")?, at("took_s")?, at("action")?);
    let (c_ref, c_feeder, c_nozzle, c_result) = (
        at("reference")?,
        at("feeder")?,
        at("nozzle")?,
        at("result")?,
    );
    let c_status = keys.iter().position(|k| k == "status");
    let c_state = keys.iter().position(|k| k == "nozzle_state");
    let mut out = Vec::new();
    for (line, row) in rows {
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        if out.len() >= MAX_STEPS {
            return Err(bad(line, format!("more than {MAX_STEPS} steps")));
        }
        let cell = |i: usize| row.get(i).map_or("", |s| s.trim());
        let seconds = |i: usize, what: &str| -> Result<f64, PnpError> {
            cell(i)
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && *v >= 0.0 && *v < 1e12)
                .ok_or_else(|| bad(line, format!("{what} is a number of seconds, 0 or more")))
        };
        let whole = |i: usize, what: &str| -> Result<usize, PnpError> {
            cell(i)
                .parse::<usize>()
                .ok()
                .filter(|v| *v <= 1_000_000_000)
                .ok_or_else(|| bad(line, format!("{what} is a whole number")))
        };
        let nozzle_state = c_state.map_or("", cell).to_string();
        if !nozzle_state.is_empty() && NozzleState::parse(&nozzle_state).is_none() {
            return Err(bad(
                line,
                format!("'{nozzle_state}' is not a state of a nozzle"),
            ));
        }
        out.push(Step {
            step: whole(c_step, "step")?,
            at_s: seconds(c_at, "at_s")?,
            took_s: seconds(c_took, "took_s")?,
            action: cell(c_action).to_string(),
            reference: cell(c_ref).to_string(),
            feeder: cell(c_feeder).to_string(),
            nozzle: whole(c_nozzle, "nozzle")?,
            result: cell(c_result).to_string(),
            status: c_status.map_or("", cell).to_string(),
            nozzle_state,
        });
    }
    if out.is_empty() {
        return Err(bad(hline, "the journal has no steps"));
    }
    Ok(out)
}

/// How busy a nozzle or a feeder was.
#[derive(Debug, Clone, PartialEq)]
pub struct Use {
    pub name: String,
    pub busy_s: f64,
    pub steps: usize,
    /// The share of the time of the job, from 0 to 1.
    pub share: f64,
}

/// A stretch in which no step ran.
#[derive(Debug, Clone, PartialEq)]
pub struct Gap {
    /// The step after which the machine waited.
    pub after_step: usize,
    pub from_s: f64,
    pub length_s: f64,
}

/// What the journal says about the use of the machine.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Busy {
    /// From the start of the first step to the end of the last.
    pub total_s: f64,
    pub nozzles: Vec<Use>,
    pub feeders: Vec<Use>,
    /// The longest gaps, the longest first.
    pub gaps: Vec<Gap>,
    /// The seconds in all the gaps.
    pub waiting_s: f64,
    /// The steps that did not end with `ok`.
    pub failed: usize,
}

/// Counts the use of the nozzles and the feeders and finds the gaps; `listed` is how many gaps are kept.
pub fn analyse(steps: &[Step], listed: usize) -> Busy {
    let mut b = Busy::default();
    let Some(first) = steps.first() else {
        return b;
    };
    let end = steps
        .iter()
        .map(|s| s.at_s + s.took_s)
        .fold(first.at_s, f64::max);
    b.total_s = (end - first.at_s).max(0.0);
    let mut nozzles: BTreeMap<usize, (f64, usize)> = BTreeMap::new();
    let mut feeders: BTreeMap<String, (f64, usize)> = BTreeMap::new();
    for s in steps {
        let n = nozzles.entry(s.nozzle).or_default();
        n.0 += s.took_s;
        n.1 += 1;
        if s.action == "pick" {
            let f = feeders.entry(s.feeder.clone()).or_default();
            f.0 += s.took_s;
            f.1 += 1;
        }
        if s.result != "ok" {
            b.failed += 1;
        }
    }
    let share = |busy: f64| {
        if b.total_s > 0.0 {
            (busy / b.total_s).min(1.0)
        } else {
            0.0
        }
    };
    b.nozzles = nozzles
        .into_iter()
        .map(|(n, (busy, count))| Use {
            name: n.to_string(),
            busy_s: busy,
            steps: count,
            share: share(busy),
        })
        .collect();
    b.feeders = feeders
        .into_iter()
        .map(|(n, (busy, count))| Use {
            name: n,
            busy_s: busy,
            steps: count,
            share: share(busy),
        })
        .collect();
    let mut gaps = Vec::new();
    for pair in steps.windows(2) {
        let finished = pair[0].at_s + pair[0].took_s;
        let wait = pair[1].at_s - finished;
        if wait > 1e-9 {
            b.waiting_s += wait;
            gaps.push(Gap {
                after_step: pair[0].step,
                from_s: finished,
                length_s: wait,
            });
        }
    }
    gaps.sort_by(|a, c| {
        c.length_s
            .total_cmp(&a.length_s)
            .then_with(|| a.after_step.cmp(&c.after_step))
    });
    gaps.truncate(listed);
    b.gaps = gaps;
    b
}

impl Busy {
    /// The report for a person.
    pub fn text(&self) -> String {
        let mut out = format!(
            "{:.3} s from the first step to the last, {:.3} s of waiting between steps, {} steps that did not end well\n",
            self.total_s, self.waiting_s, self.failed
        );
        for u in &self.nozzles {
            out.push_str(&format!(
                "nozzle {}: {:.3} s in {} steps, {:.1} % of the job\n",
                u.name,
                u.busy_s,
                u.steps,
                u.share * 100.0
            ));
        }
        for u in &self.feeders {
            out.push_str(&format!(
                "feeder {}: {:.3} s in {} picks, {:.1} % of the job\n",
                u.name,
                u.busy_s,
                u.steps,
                u.share * 100.0
            ));
        }
        for g in &self.gaps {
            out.push_str(&format!(
                "the machine waited {:.3} s after step {} (from second {:.3})\n",
                g.length_s, g.after_step, g.from_s
            ));
        }
        out
    }

    /// The use as a CSV: `kind,name,busy_s,steps,share`.
    pub fn csv(&self) -> String {
        let mut out = String::from("kind,name,busy_s,steps,share\n");
        for (kind, list) in [("nozzle", &self.nozzles), ("feeder", &self.feeders)] {
            for u in list {
                out.push_str(&format!(
                    "{kind},{},{:.3},{},{:.4}\n",
                    csv_field(&u.name),
                    u.busy_s,
                    u.steps,
                    u.share
                ));
            }
        }
        out
    }
}
