// EH3D-PHOENIX-PNP - how much time each number of nozzles on the head would save.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A head with more nozzles picks more parts before it goes to the board, but each nozzle is a cost (a tip to keep, a head that is heavier) and the gain is smaller with each one. Before a
//! person decides how many nozzles a head needs, or whether the second head is worth a plan, this plans the same board with one nozzle, then two, up to six, and says for each the batches,
//! the changes of tip, the way the head travels and the seconds the plan takes, the fastest marked and the time each nozzle takes off the plan of one nozzle. The heads that the
//! configuration declares are left out of the count (the comparison is about one head with that many nozzles) and the line says so. Nothing here changes a configuration.

use crate::catalog::Catalog;
use crate::config::MachineConfig;
use crate::feeding::{FeederSpec, Picks};
use crate::parts::{csv_field, Part};
use crate::planner::{plan_advanced, Options};

/// The most nozzles of a head.
pub const MAX_NOZZLES: usize = 6;

/// The plan of one number of nozzles.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub nozzles: usize,
    /// The plan, or why it could not be made.
    pub plan: Result<Figures, String>,
}

/// What a plan takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Figures {
    pub batches: usize,
    pub tip_changes: usize,
    pub travel_mm: f64,
    pub seconds: f64,
}

/// The board planned with 1 to `most` nozzles (`most` is held to 1 to 6) on one head, the tips and the order as the configuration has them.
pub fn compare(
    parts: &[Part],
    feeders: &[FeederSpec],
    picks: &Picks,
    catalog: &Catalog,
    cfg: &MachineConfig,
    opts: &Options,
    most: usize,
) -> Vec<Row> {
    let most = most.clamp(1, MAX_NOZZLES);
    (1..=most)
        .map(|n| {
            let mut c = cfg.clone();
            c.heads = Vec::new();
            c.nozzles = n;
            let plan = plan_advanced(parts, feeders, picks, catalog, &c, opts)
                .map(|p| Figures {
                    batches: p.plan.batches,
                    tip_changes: p.tip_changes,
                    travel_mm: p.plan.travel_mm,
                    seconds: p.plan.seconds,
                })
                .map_err(|e| e.to_string());
            Row { nozzles: n, plan }
        })
        .collect()
}

/// The row of the fastest plan: the fewest seconds, then the fewest nozzles. `None` when no plan could be made.
pub fn fastest(rows: &[Row]) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (i, r) in rows.iter().enumerate() {
        let Ok(f) = &r.plan else {
            continue;
        };
        let better = match best {
            None => true,
            Some(b) => rows[b]
                .plan
                .as_ref()
                .is_ok_and(|cur| f.seconds < cur.seconds),
        };
        if better {
            best = Some(i);
        }
    }
    best
}

/// The seconds that a nozzle takes off the plan of the nozzle before it (`None` for the first, or when either plan could not be made).
pub fn gain_s(rows: &[Row], i: usize) -> Option<f64> {
    let before = rows.get(i.checked_sub(1)?)?.plan.as_ref().ok()?;
    let this = rows.get(i)?.plan.as_ref().ok()?;
    Some(before.seconds - this.seconds)
}

/// The rows as text; `declared_heads` is how many heads the configuration declares (they were set aside) and `now` the nozzles of the configuration.
pub fn text(rows: &[Row], now: usize, declared_heads: usize) -> String {
    let best = fastest(rows);
    let mut out = String::new();
    if declared_heads > 0 {
        out.push_str(&format!(
            "the {declared_heads} heads of the configuration are left out: one head with each number of nozzles is planned\n"
        ));
    }
    out.push_str(&format!(
        "  {:<7} {:>7} {:>5} {:>10} {:>9} {:>9}\n",
        "nozzles", "batches", "tips", "travel mm", "seconds", "gain s"
    ));
    for (i, r) in rows.iter().enumerate() {
        match &r.plan {
            Ok(f) => out.push_str(&format!(
                "  {:<7} {:>7} {:>5} {:>10.1} {:>9.1} {:>9}{}{}\n",
                r.nozzles,
                f.batches,
                f.tip_changes,
                f.travel_mm,
                f.seconds,
                gain_s(rows, i).map_or("-".to_string(), |g| format!("{g:.1}")),
                if r.nozzles == now {
                    "  <- the configuration"
                } else {
                    ""
                },
                if best == Some(i) { "  <- fastest" } else { "" }
            )),
            Err(why) => out.push_str(&format!("  {:<7} no plan: {why}\n", r.nozzles)),
        }
    }
    match best {
        Some(i) => {
            let r = &rows[i];
            let at_now = rows
                .iter()
                .find(|x| x.nozzles == now)
                .and_then(|x| x.plan.as_ref().ok());
            let here = r.plan.as_ref().ok();
            match (here, at_now) {
                (Some(h), Some(n)) if r.nozzles != now && n.seconds > h.seconds => {
                    out.push_str(&format!(
                        "{} nozzles would take {:.1} s off the {:.1} s of the plan with the {} nozzles of the configuration\n",
                        r.nozzles,
                        n.seconds - h.seconds,
                        n.seconds,
                        now
                    ));
                }
                _ => out.push_str(&format!(
                    "the fastest plan is the one with {} nozzles\n",
                    r.nozzles
                )),
            }
        }
        None => out.push_str("no plan could be made with any number of nozzles\n"),
    }
    out
}

/// The rows as a CSV: `nozzles,batches,tip_changes,travel_mm,seconds,gain_s,error`.
pub fn csv(rows: &[Row]) -> String {
    let mut out = String::from("nozzles,batches,tip_changes,travel_mm,seconds,gain_s,error\n");
    for (i, r) in rows.iter().enumerate() {
        match &r.plan {
            Ok(f) => out.push_str(&format!(
                "{},{},{},{:.3},{:.3},{},\n",
                r.nozzles,
                f.batches,
                f.tip_changes,
                f.travel_mm,
                f.seconds,
                gain_s(rows, i).map_or(String::new(), |g| format!("{g:.3}"))
            )),
            Err(why) => out.push_str(&format!("{},,,,,,{}\n", r.nozzles, csv_field(why))),
        }
    }
    out
}
