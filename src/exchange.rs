// EH3D-PHOENIX-PNP - a job in a file that other programs can read: the plan and where the job is.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A plan that is written for another program (a viewer, a script, a second machine, a spreadsheet) needs more than the places: it needs what has become of each part. This file is the plan and the
//! state of a job in one JSON document, with a version and a kind so that nobody takes it for something else:
//!
//! ```text
//! { "schema_version": 1, "kind": "pnp-job-exchange",
//!   "plan":  { "order", "batches", "tip_changes", "tiers", "travel_mm", "seconds", "placements": [ { "reference", "feeder", "pick": {x, y}, "place": {x, y, rotation},
//!              "nozzle", "batch", "rank", "tier", "tip", "bin" } ] },
//!   "state": { "phase", "parts": [ { "reference", "status", "reason" } ] } }
//! ```
//!
//! `plan` is the document of `schedule` (its `placements` as they are in the plan), so a program that reads one reads the other. `state` has a row for each placement, in the same order;
//! `status` is `pending`, `placed`, `deferred` (put aside) or `skipped` (left out by the person). Writing one never moves a machine. Reading one is careful, because the file comes from outside:
//! it has a size limit and a limit of placements, every number is finite and within 100000 millimetres, a name or a reason has a length and no control characters, a reference is in the plan
//! once, the batches and the nozzles are counted, the state has a row for each placement and names the same reference, and the phase and the statuses are the ones above and agree with
//! each other (a job that is finished has nothing pending, and one that is ready has nothing placed that is not a placement of the plan). A file that is not understood is an error with its
//! reason and the code `PHOENIX-PNP-026`; nothing is run from it, and nothing of it is applied to a machine.

use crate::error::{Code, PnpError};
use crate::job::{Job, Phase, Status};
use crate::planner::AdvancedPlan;
use serde_json::{json, Value};

/// The version of the file.
pub const SCHEMA_VERSION: u64 = 1;
/// The kind of the file.
pub const KIND: &str = "pnp-job-exchange";
/// The most placements of one file.
pub const MAX_PLACEMENTS: usize = 100_000;
/// The largest coordinate, in millimetres.
const MAX_COORD: f64 = 100_000.0;
/// The most characters of a name and of a reason.
const MAX_NAME: usize = 128;
const MAX_REASON: usize = 512;
/// The most nozzles, batches and tiers that are believed.
const MAX_NOZZLES: u64 = 64;

fn bad(detail: impl Into<String>) -> PnpError {
    PnpError::new(Code::JobStateBad, detail)
}

/// The name of a phase as the file writes it.
pub fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Ready => "ready",
        Phase::Paused => "paused",
        Phase::Stopped => "stopped",
        Phase::Finished => "finished",
    }
}

fn phase_of(text: &str) -> Option<Phase> {
    [Phase::Ready, Phase::Paused, Phase::Stopped, Phase::Finished]
        .into_iter()
        .find(|p| phase_name(*p) == text)
}

/// The plan alone as a file: every part is pending and the job is ready.
pub fn from_plan(plan: &AdvancedPlan) -> String {
    let statuses = vec![Status::Pending; plan.plan.placements.len()];
    let reasons = vec![String::new(); plan.plan.placements.len()];
    document(plan, Phase::Ready, &statuses, &reasons)
}

/// A job as a file: its plan and where it is.
pub fn from_job(job: &Job) -> String {
    let reasons: Vec<String> = (0..job.status().len())
        .map(|i| job.reason(i).to_string())
        .collect();
    document(job.plan(), job.phase(), job.status(), &reasons)
}

fn document(plan: &AdvancedPlan, phase: Phase, statuses: &[Status], reasons: &[String]) -> String {
    // the plan is the document of `schedule`
    let mut plan_doc: Value = serde_json::from_str(&plan.to_json()).unwrap_or(Value::Null);
    if let Value::Object(map) = &mut plan_doc {
        map.remove("schema_version");
    }
    let parts: Vec<Value> = plan
        .plan
        .placements
        .iter()
        .zip(statuses)
        .zip(reasons)
        .map(|((p, s), why)| json!({"reference": p.reference, "status": s.name(), "reason": why}))
        .collect();
    let doc = json!({
        "schema_version": SCHEMA_VERSION,
        "kind": KIND,
        "plan": plan_doc,
        "state": {"phase": phase_name(phase), "parts": parts}
    });
    serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n"
}

/// One placement of the plan with what became of it.
#[derive(Debug, Clone, PartialEq)]
pub struct ExchangePart {
    pub reference: String,
    pub feeder: String,
    pub pick: (f64, f64),
    pub place: (f64, f64),
    pub rotation: f64,
    pub nozzle: usize,
    pub batch: usize,
    pub rank: i64,
    pub tier: i64,
    pub tip: String,
    pub bin: u64,
    pub status: Status,
    pub reason: String,
}

/// A job read from a file.
#[derive(Debug, Clone, PartialEq)]
pub struct Exchange {
    pub order: String,
    pub batches: usize,
    pub tip_changes: u64,
    pub tiers: u64,
    pub travel_mm: f64,
    pub seconds: f64,
    pub phase: Phase,
    pub parts: Vec<ExchangePart>,
}

fn clean(text: &str, max: usize) -> bool {
    text.chars().count() <= max && !text.chars().any(char::is_control)
}

fn text_of(v: &Value, key: &str, max: usize, what: &str) -> Result<String, PnpError> {
    let t = v
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| bad(format!("{what}: {key} is not a text")))?;
    if !clean(t, max) {
        return Err(bad(format!(
            "{what}: {key} is longer than {max} characters or has a control character"
        )));
    }
    Ok(t.to_string())
}

fn number(v: Option<&Value>, what: &str) -> Result<f64, PnpError> {
    v.and_then(Value::as_f64)
        .filter(|n| n.is_finite() && n.abs() <= MAX_COORD)
        .ok_or_else(|| bad(format!("{what} is not a number within {MAX_COORD} mm")))
}

fn count(v: Option<&Value>, what: &str, max: u64) -> Result<u64, PnpError> {
    v.and_then(Value::as_u64)
        .filter(|n| *n <= max)
        .ok_or_else(|| bad(format!("{what} is not a whole number from 0 to {max}")))
}

fn whole(v: Option<&Value>, what: &str) -> Result<i64, PnpError> {
    v.and_then(Value::as_i64)
        .filter(|n| n.abs() <= 1_000_000_000)
        .ok_or_else(|| bad(format!("{what} is not a whole number")))
}

/// Reads the file of a job. See the top of the module for what is checked.
pub fn parse(text: &str) -> Result<Exchange, PnpError> {
    let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| bad(format!("not valid JSON: {e}")))?;
    match v.get("schema_version").and_then(Value::as_u64) {
        Some(SCHEMA_VERSION) => {}
        Some(n) if n > SCHEMA_VERSION => {
            return Err(bad(format!(
                "the file is of version {n}, newer than this program reads ({SCHEMA_VERSION})"
            )))
        }
        _ => return Err(bad("schema_version is not 1")),
    }
    if v.get("kind").and_then(Value::as_str) != Some(KIND) {
        return Err(bad(format!(
            "this is not a job exchange file (kind is not {KIND})"
        )));
    }
    let plan = v.get("plan").ok_or_else(|| bad("no plan"))?;
    let state = v.get("state").ok_or_else(|| bad("no state"))?;
    let placements = plan
        .get("placements")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("the plan has no list of placements"))?;
    if placements.len() > MAX_PLACEMENTS {
        return Err(bad(format!("more than {MAX_PLACEMENTS} placements")));
    }
    let rows = state
        .get("parts")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("the state has no list of parts"))?;
    if rows.len() != placements.len() {
        return Err(bad(format!(
            "the state has {} parts and the plan has {} placements",
            rows.len(),
            placements.len()
        )));
    }
    let phase = state
        .get("phase")
        .and_then(Value::as_str)
        .and_then(phase_of)
        .ok_or_else(|| bad("the phase is not ready, paused, stopped or finished"))?;
    let batches = count(plan.get("batches"), "batches", MAX_PLACEMENTS as u64)? as usize;
    let mut parts: Vec<ExchangePart> = Vec::with_capacity(placements.len());
    let mut seen = std::collections::HashSet::new();
    for (i, (p, s)) in placements.iter().zip(rows).enumerate() {
        let what = format!("placement {}", i + 1);
        let reference = text_of(p, "reference", MAX_NAME, &what)?;
        if reference.trim().is_empty() {
            return Err(bad(format!("{what}: the reference is empty")));
        }
        if !seen.insert(reference.clone()) {
            return Err(bad(format!(
                "{what}: the reference '{reference}' is in the plan twice"
            )));
        }
        let xy = |key: &str| -> Result<(f64, f64), PnpError> {
            let o = p.get(key).ok_or_else(|| bad(format!("{what}: no {key}")))?;
            Ok((
                number(o.get("x"), &format!("{what}: {key} x"))?,
                number(o.get("y"), &format!("{what}: {key} y"))?,
            ))
        };
        let place = xy("place")?;
        let rotation = number(
            p.get("place").and_then(|o| o.get("rotation")),
            &format!("{what}: the rotation"),
        )?;
        let nozzle = count(p.get("nozzle"), &format!("{what}: nozzle"), MAX_NOZZLES - 1)? as usize;
        let batch = count(
            p.get("batch"),
            &format!("{what}: batch"),
            MAX_PLACEMENTS as u64,
        )? as usize;
        if batch >= batches.max(1) {
            return Err(bad(format!(
                "{what}: batch {batch} is outside the {batches} batches of the plan"
            )));
        }
        let row_reference = text_of(s, "reference", MAX_NAME, &format!("part {}", i + 1))?;
        if row_reference != reference {
            return Err(bad(format!(
                "part {} of the state is '{row_reference}' and placement {} of the plan is '{reference}'",
                i + 1,
                i + 1
            )));
        }
        let status = s
            .get("status")
            .and_then(Value::as_str)
            .and_then(Status::parse)
            .ok_or_else(|| {
                bad(format!(
                    "part {}: the status is not pending, placed, deferred or skipped",
                    i + 1
                ))
            })?;
        let reason = text_of(s, "reason", MAX_REASON, &format!("part {}", i + 1))?;
        parts.push(ExchangePart {
            feeder: text_of(p, "feeder", MAX_NAME, &what)?,
            pick: xy("pick")?,
            place,
            rotation,
            nozzle,
            batch,
            rank: whole(p.get("rank"), &format!("{what}: rank"))?,
            tier: whole(p.get("tier"), &format!("{what}: tier"))?,
            tip: text_of(p, "tip", MAX_NAME, &what)?,
            bin: count(p.get("bin"), &format!("{what}: bin"), 255)?,
            status,
            reason,
            reference,
        });
    }
    // the phase and the statuses agree
    let pending = parts.iter().filter(|p| p.status == Status::Pending).count();
    if phase == Phase::Finished && pending > 0 {
        return Err(bad(format!(
            "the job is finished but {pending} parts are still pending"
        )));
    }
    if phase == Phase::Ready && parts.iter().any(|p| p.status == Status::Placed) {
        return Err(bad(
            "the job is ready to start but a part is already placed",
        ));
    }
    Ok(Exchange {
        order: text_of(plan, "order", MAX_NAME, "the plan")?,
        batches,
        tip_changes: count(
            plan.get("tip_changes"),
            "tip_changes",
            MAX_PLACEMENTS as u64,
        )?,
        tiers: count(plan.get("tiers"), "tiers", MAX_PLACEMENTS as u64)?,
        travel_mm: number(plan.get("travel_mm"), "travel_mm")?,
        seconds: number(plan.get("seconds"), "seconds")?,
        phase,
        parts,
    })
}

impl Exchange {
    /// How many parts have each status: pending, placed, deferred, skipped.
    pub fn counts(&self) -> [usize; 4] {
        let mut c = [0; 4];
        for p in &self.parts {
            c[match p.status {
                Status::Pending => 0,
                Status::Placed => 1,
                Status::Deferred => 2,
                Status::Skipped => 3,
            }] += 1;
        }
        c
    }

    /// What the file holds, in lines for a person.
    pub fn summary(&self) -> String {
        let c = self.counts();
        format!(
            "{} parts in {} batches, order {}, travel {:.1} mm, about {:.1} s, the job is {}\n{} placed, {} pending, {} put aside, {} left out",
            self.parts.len(),
            self.batches,
            self.order,
            self.travel_mm,
            self.seconds,
            phase_name(self.phase),
            c[1],
            c[0],
            c[2],
            c[3]
        )
    }

    /// The placements as a table: `reference,feeder,x,y,rotation,nozzle,batch,tip,status,reason`.
    pub fn csv(&self) -> String {
        let field = crate::parts::csv_field;
        let mut out =
            String::from("reference,feeder,x,y,rotation,nozzle,batch,tip,status,reason\n");
        for p in &self.parts {
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{}\n",
                field(&p.reference),
                field(&p.feeder),
                p.place.0,
                p.place.1,
                p.rotation,
                p.nozzle + 1,
                p.batch + 1,
                field(&p.tip),
                p.status.name(),
                field(&p.reason)
            ));
        }
        out
    }
}
