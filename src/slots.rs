// EH3D-PHOENIX-PNP - where the feeders stand so that the plan is the fastest.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The head goes to the feeders for every part, and a feeder that holds the part of a hundred placements stands best near the board. The table of feeders says where each one stands; this
//! looks for a better way to stand them. It swaps the places of two feeders of the same kind (tape with tape, tube with tube, bulk with bulk, both switched on; a strip and a tray are
//! tied to their own geometry and stay where they are), plans the board again with each swap and keeps the swap that makes the plan the fastest, until no swap makes it faster or the number
//! of plans that was allowed is used up. The feeders only trade places, so no place is new and none is used twice; whether a tape fits the slot it is moved to is not known (the table does
//! not say how wide a slot is) and is for the person to look at. The slot names go with the places. Everything else in the table is kept, and the result is the same for the same input.

use crate::catalog::Catalog;
use crate::config::MachineConfig;
use crate::deplete::{csv_cell, place};
use crate::error::{Code, PnpError};
use crate::feeding::{assign, from_csv_with, FeederKind, FeederSpec};
use crate::parts::{detect_delimiter, read_records, Part};
use crate::planner::{plan_advanced, Options};

/// The fewest seconds that a swap has to save to be kept.
const MIN_GAIN_S: f64 = 1e-6;
/// The most rounds of swaps.
const MAX_ROUNDS: usize = 200;

/// A feeder that was moved.
#[derive(Debug, Clone, PartialEq)]
pub struct Move {
    pub id: String,
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub slot_from: Option<String>,
    pub slot_to: Option<String>,
}

/// The feeders as they are best placed, and what that saves.
#[derive(Debug, Clone, PartialEq)]
pub struct Slots {
    pub moves: Vec<Move>,
    pub before_s: f64,
    pub after_s: f64,
    pub before_mm: f64,
    pub after_mm: f64,
    /// How many plans were made to find it (the first one included).
    pub plans: usize,
    /// The search stopped because the plans that were allowed were used up.
    pub truncated: bool,
    /// The table of feeders with the new places (and slot names, when the table has them).
    pub csv: String,
}

fn bad(message: impl Into<String>) -> PnpError {
    PnpError::new(Code::PlanSetting, message)
}

/// The seconds and the travel of the plan of `parts` with the feeders as they are, or why there is none.
fn measure(
    parts: &[Part],
    feeders: &[FeederSpec],
    catalog: &Catalog,
    cfg: &MachineConfig,
    options: &Options,
) -> Result<(f64, f64), PnpError> {
    let picks = assign(parts, feeders, Some(options.side));
    let plan = plan_advanced(parts, feeders, &picks, catalog, cfg, options)?;
    Ok((plan.plan.seconds, plan.plan.travel_mm))
}

/// The kinds of feeder whose places may be traded, as a number for each kind.
fn group_of(spec: &FeederSpec) -> Option<u8> {
    if !spec.enabled {
        return None;
    }
    match spec.kind {
        FeederKind::Tape => Some(0),
        FeederKind::Tube => Some(1),
        FeederKind::Bulk => Some(2),
        FeederKind::Tray { .. } | FeederKind::Strip { .. } => None,
    }
}

fn swap_places(feeders: &mut [FeederSpec], i: usize, j: usize) {
    let (xi, yi, si) = (
        feeders[i].base.x,
        feeders[i].base.y,
        feeders[i].slot.clone(),
    );
    feeders[i].base.x = feeders[j].base.x;
    feeders[i].base.y = feeders[j].base.y;
    feeders[i].slot = feeders[j].slot.clone();
    feeders[j].base.x = xi;
    feeders[j].base.y = yi;
    feeders[j].slot = si;
}

/// Looks for the places of the feeders of `feeders_text` that make the plan of `parts` the fastest, with at most `max_plans` plans. The plan of the table as it stands has to be possible.
pub fn optimize(
    parts: &[Part],
    feeders_text: &str,
    catalog: &Catalog,
    cfg: &MachineConfig,
    options: &Options,
    max_plans: usize,
) -> Result<Slots, PnpError> {
    if max_plans == 0 {
        return Err(bad("the number of plans must be at least 1"));
    }
    let original = from_csv_with(feeders_text, cfg.tray_fill)?;
    let mut feeders = original.clone();
    let (before_s, before_mm) = measure(parts, &feeders, catalog, cfg, options)?;
    let mut plans = 1usize;
    let (mut best_s, mut best_mm) = (before_s, before_mm);
    let mut truncated = false;
    'rounds: for _ in 0..MAX_ROUNDS {
        let mut found: Option<(usize, usize, f64, f64)> = None;
        for i in 0..feeders.len() {
            let Some(group) = group_of(&feeders[i]) else {
                continue;
            };
            for j in i + 1..feeders.len() {
                if group_of(&feeders[j]) != Some(group) {
                    continue;
                }
                // two feeders that stand in the same place gain nothing from trading
                if feeders[i].base.x == feeders[j].base.x && feeders[i].base.y == feeders[j].base.y
                {
                    continue;
                }
                if plans >= max_plans {
                    truncated = true;
                    break 'rounds;
                }
                swap_places(&mut feeders, i, j);
                plans += 1;
                let tried = measure(parts, &feeders, catalog, cfg, options);
                swap_places(&mut feeders, i, j);
                let Ok((s, mm)) = tried else {
                    continue;
                };
                let (bar_s, bar_mm) = found.map_or((best_s, best_mm), |f| (f.2, f.3));
                if s < bar_s - MIN_GAIN_S
                    || (found.is_some() && (s - bar_s).abs() <= MIN_GAIN_S && mm < bar_mm - 1e-6)
                {
                    found = Some((i, j, s, mm));
                }
            }
        }
        match found {
            Some((i, j, s, mm)) => {
                swap_places(&mut feeders, i, j);
                (best_s, best_mm) = (s, mm);
            }
            None => break,
        }
    }
    let moves: Vec<Move> = original
        .iter()
        .zip(&feeders)
        .filter(|(a, b)| a.base.x != b.base.x || a.base.y != b.base.y)
        .map(|(a, b)| Move {
            id: a.base.id.clone(),
            from: (a.base.x, a.base.y),
            to: (b.base.x, b.base.y),
            slot_from: a.slot.clone(),
            slot_to: b.slot.clone(),
        })
        .collect();
    // the table, with the places and the slot names of the feeders that moved
    let text = feeders_text.trim_start_matches('\u{feff}');
    let head_line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let delim = detect_delimiter(head_line);
    let mut records = read_records(text, delim).map_err(bad)?;
    let header: Vec<String> = records[0].iter().map(|h| h.to_ascii_lowercase()).collect();
    let col = |name: &str| header.iter().position(|h| h == name);
    let (Some(c_x), Some(c_y), c_slot) = (col("x"), col("y"), col("slot")) else {
        return Err(bad("the table of feeders has no columns x and y"));
    };
    for (i, (a, b)) in original.iter().zip(&feeders).enumerate() {
        if a.base.x == b.base.x && a.base.y == b.base.y {
            continue;
        }
        let row = &mut records[i + 1];
        row[c_x] = place(b.base.x);
        row[c_y] = place(b.base.y);
        if let Some(c) = c_slot {
            row[c] = b.slot.clone().unwrap_or_default();
        }
    }
    let sep = delim.to_string();
    let mut csv = String::new();
    for r in &records {
        let cells: Vec<String> = r.iter().map(|c| csv_cell(c, delim)).collect();
        csv.push_str(&cells.join(&sep));
        csv.push('\n');
    }
    Ok(Slots {
        moves,
        before_s,
        after_s: best_s,
        before_mm,
        after_mm: best_mm,
        plans,
        truncated,
        csv,
    })
}

impl Slots {
    /// How many seconds the new places save on the board.
    pub fn saved_s(&self) -> f64 {
        (self.before_s - self.after_s).max(0.0)
    }

    /// One line for each feeder that moves, then what it saves.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for m in &self.moves {
            out.push_str(&format!(
                "{}: {:.3},{:.3} -> {:.3},{:.3}",
                m.id, m.from.0, m.from.1, m.to.0, m.to.1
            ));
            if m.slot_from != m.slot_to {
                out.push_str(&format!(
                    " (slot {} -> {})",
                    m.slot_from.as_deref().unwrap_or("-"),
                    m.slot_to.as_deref().unwrap_or("-")
                ));
            }
            out.push('\n');
        }
        if self.moves.is_empty() {
            out.push_str(&format!(
                "the feeders are well placed: no swap of two feeders makes the plan faster ({} plans tried{})\n",
                self.plans,
                if self.truncated { ", the search was cut" } else { "" }
            ));
        } else {
            out.push_str(&format!(
                "{} feeders move: the plan takes {:.1} s instead of {:.1} s ({:.1} s less) and the head travels {:.1} mm instead of {:.1} mm, after {} plans{}\n",
                self.moves.len(),
                self.after_s,
                self.before_s,
                self.saved_s(),
                self.after_mm,
                self.before_mm,
                self.plans,
                if self.truncated { " (the search was cut)" } else { "" }
            ));
            out.push_str(
                "look at each tape before it is moved: the table does not say how wide a slot is\n",
            );
        }
        out
    }
}
