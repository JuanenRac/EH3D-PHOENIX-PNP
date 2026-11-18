// EH3D-PHOENIX-PNP - how much each nozzle tip has been used.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The use of the nozzle tips: how many parts each tip has placed, how many times it was mounted and how many times it was replaced, kept in a small CSV that the person keeps beside the
//! configuration of the machine, so that a worn tip is noticed before it drops parts. The file is only ever changed on purpose: by `simulate --nozzle-life` after a job that ran to its end
//! (a job is counted once, whatever number of times its saved state is run again), or by `nozzle-life --reset` when a tip was replaced. A file that is not a file of the use of nozzle tips
//! is never written over, the counts never go down but by a reset, and the file is written whole to a temporary name and then moved into place, with the file it replaces kept as
//! `FILE.prev`.

use crate::error::{Code, PnpError};
use crate::job::{Job, Status};
use crate::parts::{csv_field, detect_delimiter, read_records};
use std::collections::BTreeMap;
use std::path::Path;

/// The most tips in a file.
pub const MAX_TIPS: usize = 1000;
/// The header of the file.
pub const HEADER: &str = "tip,uses,mounts,replaced";
/// The largest count of a tip.
pub const MAX_COUNT: u64 = 1_000_000_000_000;
const LAST_JOB: &str = "# last_job = ";

/// The use of one tip.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TipLife {
    pub tip: String,
    /// The parts it has placed since it was put on the machine.
    pub uses: u64,
    /// The times it was mounted on a nozzle.
    pub mounts: u64,
    /// The times it was replaced by a new one (a reset).
    pub replaced: u32,
}

/// The use of all the tips, in the order of their names.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NozzleLife {
    tips: BTreeMap<String, TipLife>,
    /// The identity of the last job that was added, so that the same job is not counted twice.
    last_job: String,
}

fn bad(message: impl Into<String>) -> PnpError {
    PnpError::new(Code::NozzleLifeBad, message)
}

/// Whether a text can be the name of a tip: 1 to 64 characters and no control character or comma.
pub fn tip_name_ok(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty()
        && n.chars().count() <= 64
        && !n.chars().any(|c| c.is_control() || c == ',' || c == '"')
}

impl NozzleLife {
    /// A file of the use of the tips: the header `tip,uses,mounts,replaced` and a row for each tip. Lines that start with `#` are notes.
    pub fn parse(text: &str) -> Result<NozzleLife, PnpError> {
        let text = text.trim_start_matches('\u{feff}');
        let mut life = NozzleLife::default();
        let mut body: Vec<&str> = Vec::new();
        for line in text.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix(LAST_JOB.trim_end()) {
                let id = rest.trim_start_matches([' ', '=']).trim();
                if id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric()) {
                    life.last_job = id.to_string();
                }
            } else if !t.is_empty() && !t.starts_with('#') {
                body.push(line);
            }
        }
        let Some(head_line) = body.first() else {
            return Err(bad("the file is empty"));
        };
        let mut records = read_records(&body.join("\n"), detect_delimiter(head_line))
            .map_err(bad)?
            .into_iter();
        let head: Vec<String> = records
            .next()
            .ok_or_else(|| bad("the file is empty"))?
            .iter()
            .map(|h| h.trim().to_ascii_lowercase())
            .collect();
        if head != ["tip", "uses", "mounts", "replaced"] {
            return Err(bad(format!("the first row must be exactly: {HEADER}")));
        }
        for (i, f) in records.enumerate() {
            let row = i + 2;
            if life.tips.len() >= MAX_TIPS {
                return Err(bad(format!("more than {MAX_TIPS} tips")));
            }
            if f.len() != 4 {
                return Err(bad(format!("row {row}: {} fields, not 4", f.len())));
            }
            let tip = f[0].trim().to_string();
            if !tip_name_ok(&tip) || life.tips.contains_key(&tip) {
                return Err(bad(format!(
                    "row {row}: the tip '{}' is empty, too long, has a character that cannot be in a name or is listed twice",
                    f[0]
                )));
            }
            let count = |cell: &str, name: &str, hi: u64| -> Result<u64, PnpError> {
                cell.trim()
                    .parse::<u64>()
                    .ok()
                    .filter(|v| *v <= hi)
                    .ok_or_else(|| {
                        bad(format!(
                            "row {row}: {name} '{cell}' is not a whole number up to {hi}"
                        ))
                    })
            };
            let entry = TipLife {
                tip: tip.clone(),
                uses: count(&f[1], "uses", MAX_COUNT)?,
                mounts: count(&f[2], "mounts", MAX_COUNT)?,
                replaced: count(&f[3], "replaced", u64::from(u32::MAX))? as u32,
            };
            life.tips.insert(tip, entry);
        }
        Ok(life)
    }

    /// The file as text, which `parse` reads back as the same record.
    pub fn to_csv(&self) -> String {
        let mut out = String::new();
        if !self.last_job.is_empty() {
            out.push_str(&format!("{LAST_JOB}{}\n", self.last_job));
        }
        out.push_str(HEADER);
        out.push('\n');
        for t in self.tips.values() {
            out.push_str(&format!(
                "{},{},{},{}\n",
                csv_field(&t.tip),
                t.uses,
                t.mounts,
                t.replaced
            ));
        }
        out
    }

    /// The tips, in the order of their names.
    pub fn tips(&self) -> impl Iterator<Item = &TipLife> {
        self.tips.values()
    }

    pub fn get(&self, tip: &str) -> Option<&TipLife> {
        self.tips.get(tip.trim())
    }

    pub fn is_empty(&self) -> bool {
        self.tips.is_empty()
    }

    /// The identity of the last job that was added ("" when none was).
    pub fn last_job(&self) -> &str {
        &self.last_job
    }

    /// Adds parts placed and mountings to a tip; a tip that is not in the record yet gets a row. False when the name cannot be the name of a tip or the file would hold too many.
    pub fn add(&mut self, tip: &str, uses: u64, mounts: u64) -> bool {
        let key = tip.trim().to_string();
        if !tip_name_ok(&key) || (!self.tips.contains_key(&key) && self.tips.len() >= MAX_TIPS) {
            return false;
        }
        let entry = self.tips.entry(key.clone()).or_insert(TipLife {
            tip: key,
            uses: 0,
            mounts: 0,
            replaced: 0,
        });
        entry.uses = entry.uses.saturating_add(uses).min(MAX_COUNT);
        entry.mounts = entry.mounts.saturating_add(mounts).min(MAX_COUNT);
        true
    }

    /// A tip was replaced by a new one: its uses and mountings go back to 0 and the replacements go up by one. False when the tip is not in the record.
    pub fn reset(&mut self, tip: &str) -> bool {
        match self.tips.get_mut(tip.trim()) {
            Some(t) => {
                t.uses = 0;
                t.mounts = 0;
                t.replaced = t.replaced.saturating_add(1);
                true
            }
            None => false,
        }
    }

    /// The tips that have placed `warn_at` parts or more (none when `warn_at` is 0).
    pub fn over(&self, warn_at: u64) -> Vec<&TipLife> {
        if warn_at == 0 {
            return Vec::new();
        }
        self.tips.values().filter(|t| t.uses >= warn_at).collect()
    }

    /// One line for each tip over the limit.
    pub fn warning_lines(&self, warn_at: u64) -> Vec<String> {
        self.over(warn_at)
            .into_iter()
            .map(|t| {
                format!(
                    "warning: nozzle tip {} has placed {} parts, at or over the {warn_at} that nozzle_life_warn allows: look at the tip",
                    t.tip, t.uses
                )
            })
            .collect()
    }

    /// The record as text for a person, a line for each tip; a tip over the limit says so.
    pub fn text(&self, warn_at: u64) -> String {
        let mut out = String::new();
        for t in self.tips.values() {
            out.push_str(&format!(
                "tip {}: {} parts placed, mounted {} times, replaced {} times{}\n",
                t.tip,
                t.uses,
                t.mounts,
                t.replaced,
                if warn_at > 0 && t.uses >= warn_at {
                    " <- look at this tip"
                } else {
                    ""
                }
            ));
        }
        if out.is_empty() {
            out.push_str("no tip has been used yet\n");
        }
        out
    }
}

/// What a job did with each tip: the parts it placed and the times the tip was mounted (counted over the placements in the order of the plan), by the name of the tip. A part that
/// has no tip in the plan is not counted.
pub fn uses_of_job(job: &Job) -> Vec<(String, u64, u64)> {
    let plan = job.plan();
    let mut by: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut mounted: Option<&str> = None;
    for (i, status) in job.status().iter().enumerate() {
        if *status != Status::Placed {
            continue;
        }
        let Some(detail) = plan.details.get(i) else {
            continue;
        };
        let tip = detail.tip.trim();
        if tip.is_empty() {
            continue;
        }
        let e = by.entry(tip.to_string()).or_insert((0, 0));
        e.0 += 1;
        if mounted != Some(tip) {
            e.1 += 1;
            mounted = Some(tip);
        }
    }
    by.into_iter().map(|(t, (u, m))| (t, u, m)).collect()
}

/// Reads the record at `path`. A file that is not there is an empty record; one that is there and is not a record of the use of nozzle tips is refused, and so is one larger than
/// `max_bytes`.
pub fn read_file(path: &str, max_bytes: u64) -> Result<NozzleLife, PnpError> {
    let p = Path::new(path);
    if !p.exists() {
        return Ok(NozzleLife::default());
    }
    let meta = std::fs::metadata(p).map_err(|e| bad(format!("{path}: {e}")))?;
    if meta.len() > max_bytes {
        return Err(PnpError::new(
            Code::FileTooBig,
            format!("{path} has {} bytes, the limit is {max_bytes}", meta.len()),
        ));
    }
    let text = std::fs::read_to_string(p).map_err(|e| bad(format!("{path}: {e}")))?;
    NozzleLife::parse(&text).map_err(|e| bad(format!("{path}: {}", e.detail)))
}

/// Writes the record: the file it replaces is copied to `PATH.prev`, the new text is written to `PATH.tmp` and moved into place.
pub fn write_file(path: &str, life: &NozzleLife) -> Result<(), PnpError> {
    let p = Path::new(path);
    if p.exists() {
        std::fs::copy(p, format!("{path}.prev")).map_err(|e| bad(format!("{path}.prev: {e}")))?;
    }
    let tmp = format!("{path}.tmp");
    std::fs::write(&tmp, life.to_csv())
        .and_then(|()| std::fs::rename(&tmp, p))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            bad(format!("{path}: {e}"))
        })
}

/// Adds the uses of the job `job_id` (see [`uses_of_job`]) to the record at `path`. Gives the record as it is after, or `None` when that job was the last one added and so is not
/// counted again. A record that is not valid is not touched.
pub fn update_file(
    path: &str,
    job_id: &str,
    uses: &[(String, u64, u64)],
    max_bytes: u64,
) -> Result<Option<NozzleLife>, PnpError> {
    let mut life = read_file(path, max_bytes)?;
    if !job_id.is_empty() && life.last_job == job_id {
        return Ok(None);
    }
    for (tip, n, m) in uses {
        if !life.add(tip, *n, *m) {
            return Err(bad(format!(
                "{path}: the tip '{tip}' cannot be added (a name that is not valid, or more than {MAX_TIPS} tips)"
            )));
        }
    }
    life.last_job = if job_id.chars().all(|c| c.is_ascii_alphanumeric()) && job_id.len() <= 64 {
        job_id.to_string()
    } else {
        String::new()
    };
    write_file(path, &life)?;
    Ok(Some(life))
}
