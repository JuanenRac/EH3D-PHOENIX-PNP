// EH3D-PHOENIX-PNP - whether the feeders hold enough parts for the jobs of a queue, one after the other.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! `stock` adds up the boards of one job. A day's work is a queue of jobs that share the reels of the feeders, and what a person needs to know before the first board is which reel
//! runs out, and during which job. This takes the jobs in the order of the queue, adds up what each kind of part (value and package) needs from the boards that are still to be
//! made, and says for each kind how many parts the feeders hold, how many are short and the job in which the reel runs out.

use crate::feeding::FeederSpec;
use crate::parts::{csv_field, Part};
use std::collections::BTreeMap;

/// What a job asks of the feeders: its parts (the ones of the side that is made) and the boards that are still to be made.
#[derive(Debug, Clone, PartialEq)]
pub struct JobNeed {
    pub name: String,
    pub parts: Vec<Part>,
    pub boards: u64,
}

/// One kind of part over the whole queue.
#[derive(Debug, Clone, PartialEq)]
pub struct KindNeed {
    pub value: String,
    pub package: String,
    /// The parts the jobs need, all together.
    pub needed: u64,
    /// The parts in the feeders that are switched on and hold this kind.
    pub available: u64,
    /// The parts that are missing (0 when there are enough).
    pub short: u64,
    /// The feeders that hold it, sorted.
    pub feeders: Vec<String>,
    /// The first job in which the parts run out (`None` when they last).
    pub runs_out_in: Option<String>,
    /// The part of the queue that the reel can serve: the jobs before the one in which it runs out, whole.
    pub jobs_served: usize,
}

/// The kinds of part of a queue, sorted by package and value, with what the feeders hold. The jobs are taken in order and a job needs `parts * boards` of each kind; the reel runs out in
/// the first job that takes the running total over what the feeders hold.
pub fn needs(jobs: &[JobNeed], feeders: &[FeederSpec]) -> Vec<KindNeed> {
    let mut kinds: BTreeMap<String, KindNeed> = BTreeMap::new();
    for f in feeders.iter().filter(|f| f.enabled) {
        // a feeder is listed under the kind it holds, also when no job needs it, so that the answer says what the feeders do hold
        let key = f.base.key();
        let k = kinds.entry(key).or_insert_with(|| KindNeed {
            value: f.base.value.trim().to_string(),
            package: f.base.package.trim().to_string(),
            needed: 0,
            available: 0,
            short: 0,
            feeders: Vec::new(),
            runs_out_in: None,
            jobs_served: 0,
        });
        k.available += u64::from(f.base.count);
        k.feeders.push(f.base.id.clone());
    }
    let mut running: BTreeMap<String, u64> = BTreeMap::new();
    let mut ran_out: BTreeMap<String, bool> = BTreeMap::new();
    for (j, job) in jobs.iter().enumerate() {
        let mut per_board: BTreeMap<String, (u64, &Part)> = BTreeMap::new();
        for p in &job.parts {
            per_board.entry(p.key()).or_insert((0, p)).0 += 1;
        }
        for (key, (n, p)) in per_board {
            let k = kinds.entry(key.clone()).or_insert_with(|| KindNeed {
                value: p.value.trim().to_string(),
                package: p.package.trim().to_string(),
                needed: 0,
                available: 0,
                short: 0,
                feeders: Vec::new(),
                runs_out_in: None,
                jobs_served: 0,
            });
            let take = n.saturating_mul(job.boards);
            k.needed = k.needed.saturating_add(take);
            let total = running.entry(key.clone()).or_insert(0);
            *total = total.saturating_add(take);
            if *total > k.available && !ran_out.get(&key).copied().unwrap_or(false) {
                ran_out.insert(key.clone(), true);
                k.runs_out_in = Some(job.name.clone());
                k.jobs_served = j;
            }
        }
    }
    let mut out: Vec<KindNeed> = kinds.into_values().collect();
    for k in &mut out {
        k.feeders.sort();
        k.short = k.needed.saturating_sub(k.available);
        if k.runs_out_in.is_none() {
            k.jobs_served = jobs.len();
        }
    }
    out.sort_by(|a, b| {
        (a.package.to_lowercase(), a.value.to_lowercase())
            .cmp(&(b.package.to_lowercase(), b.value.to_lowercase()))
    });
    out
}

/// Whether the feeders hold enough for every job.
pub fn is_enough(kinds: &[KindNeed]) -> bool {
    kinds.iter().all(|k| k.short == 0)
}

/// One line for each kind that falls short, then the line that says how the queue stands.
pub fn text(kinds: &[KindNeed], jobs: usize) -> String {
    let mut out = String::new();
    for k in kinds.iter().filter(|k| k.short > 0) {
        let held = if k.feeders.is_empty() {
            "no feeder holds it".to_string()
        } else {
            format!("{} in {}", k.available, k.feeders.join(", "))
        };
        out.push_str(&format!(
            "{} / {}: {} needed, {held}, {} short; it runs out in the job {} ({} jobs before it are served whole)\n",
            k.value,
            k.package,
            k.needed,
            k.short,
            k.runs_out_in.as_deref().unwrap_or("-"),
            k.jobs_served
        ));
    }
    let short = kinds.iter().filter(|k| k.short > 0).count();
    if short == 0 {
        out.push_str(&format!(
            "ok: the feeders hold enough for the {jobs} jobs ({} kinds)\n",
            kinds.iter().filter(|k| k.needed > 0).count()
        ));
    } else {
        let first = kinds
            .iter()
            .filter(|k| k.short > 0)
            .map(|k| k.jobs_served)
            .min()
            .unwrap_or(0);
        out.push_str(&format!(
            "short: {short} kinds lack {} parts for the {jobs} jobs; the feeders serve the first {first} jobs whole\n",
            kinds.iter().map(|k| k.short).sum::<u64>()
        ));
    }
    out
}

/// Every kind as a CSV: `value,package,needed,available,short,runs_out_in,feeders`.
pub fn csv(kinds: &[KindNeed]) -> String {
    let mut out = String::from("value,package,needed,available,short,runs_out_in,feeders\n");
    for k in kinds {
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            csv_field(&k.value),
            csv_field(&k.package),
            k.needed,
            k.available,
            k.short,
            csv_field(k.runs_out_in.as_deref().unwrap_or("")),
            csv_field(&k.feeders.join(" "))
        ));
    }
    out
}
