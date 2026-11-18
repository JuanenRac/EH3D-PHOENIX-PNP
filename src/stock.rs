// EH3D-PHOENIX-PNP - the parts the feeders hold against the parts a production run needs.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! Whether the tapes and the trays that are loaded are enough for a number of boards, kind of part by kind of part. A job takes the parts of one board from the feeders; this adds the
//! boards up, so that a person knows before the run which reels run out, after how many boards and by how much. Nothing here moves a machine.

use crate::error::{Code, PnpError};
use crate::feeding::FeederSpec;
use crate::parts::{csv_field, Part, Side};
use std::collections::BTreeMap;

/// The most boards of a run.
pub const MAX_STOCK_BOARDS: usize = 1_000_000;

/// One kind of part (a value and a package): how many a board uses and how many the feeders hold.
#[derive(Debug, Clone, PartialEq)]
pub struct StockLine {
    pub value: String,
    pub package: String,
    /// The parts of this kind on one board.
    pub per_board: usize,
    /// The feeders that are switched on and hold this kind, sorted.
    pub feeders: Vec<String>,
    /// The parts in those feeders.
    pub available: u64,
    /// The parts that the boards need.
    pub needed: u64,
    /// The parts that are missing (0 when there are enough).
    pub short: u64,
    /// The whole boards that the feeders can serve.
    pub boards_covered: u64,
}

/// The result for a number of boards.
#[derive(Debug, Clone, PartialEq)]
pub struct StockPlan {
    pub boards: usize,
    pub parts_per_board: usize,
    /// One line for each kind of part, sorted by package and value.
    pub lines: Vec<StockLine>,
}

/// Adds the boards up: for each kind of part of `side` (both sides for `None`) the parts that `boards` boards need, against the parts in the feeders that are switched on. The kinds are
/// the value and the package without regard to case, as the feeders hold them. A kind that no feeder holds has 0 available.
pub fn stock_plan(
    parts: &[Part],
    feeders: &[FeederSpec],
    side: Option<Side>,
    boards: usize,
) -> Result<StockPlan, PnpError> {
    if !(1..=MAX_STOCK_BOARDS).contains(&boards) {
        return Err(PnpError::new(
            Code::PlanSetting,
            format!("the number of boards must be from 1 to {MAX_STOCK_BOARDS}"),
        ));
    }
    let mut kinds: BTreeMap<String, StockLine> = BTreeMap::new();
    let mut parts_per_board = 0usize;
    for p in parts.iter().filter(|p| side.is_none_or(|s| p.side == s)) {
        parts_per_board += 1;
        kinds
            .entry(p.key())
            .or_insert_with(|| StockLine {
                value: p.value.trim().to_string(),
                package: p.package.trim().to_string(),
                per_board: 0,
                feeders: Vec::new(),
                available: 0,
                needed: 0,
                short: 0,
                boards_covered: 0,
            })
            .per_board += 1;
    }
    for f in feeders.iter().filter(|f| f.enabled) {
        if let Some(line) = kinds.get_mut(&f.base.key()) {
            line.available += u64::from(f.base.count);
            line.feeders.push(f.base.id.clone());
        }
    }
    let mut lines: Vec<StockLine> = kinds.into_values().collect();
    for l in &mut lines {
        l.feeders.sort();
        l.needed = l.per_board as u64 * boards as u64;
        l.short = l.needed.saturating_sub(l.available);
        l.boards_covered = l.available / l.per_board as u64;
    }
    lines.sort_by(|a, b| {
        (a.package.to_lowercase(), a.value.to_lowercase())
            .cmp(&(b.package.to_lowercase(), b.value.to_lowercase()))
    });
    Ok(StockPlan {
        boards,
        parts_per_board,
        lines,
    })
}

impl StockPlan {
    /// The kinds of part that the feeders do not hold enough of.
    pub fn short_kinds(&self) -> usize {
        self.lines.iter().filter(|l| l.short > 0).count()
    }

    /// The parts that are missing, all kinds together.
    pub fn short_parts(&self) -> u64 {
        self.lines.iter().map(|l| l.short).sum()
    }

    pub fn is_enough(&self) -> bool {
        self.short_kinds() == 0
    }

    /// The boards that can be made before the first reel runs out (the least of the kinds), or none when there is no part.
    pub fn boards_before_reload(&self) -> Option<u64> {
        self.lines.iter().map(|l| l.boards_covered).min()
    }

    /// One line for each kind that falls short, then a line that says how the run stands.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for l in self.lines.iter().filter(|l| l.short > 0) {
            let held = if l.feeders.is_empty() {
                "no feeder holds it".to_string()
            } else {
                format!("{} in {}", l.available, l.feeders.join(", "))
            };
            out.push_str(&format!(
                "{} / {}: {} needed ({} per board), {held}, {} short; the feeders serve {} boards\n",
                l.value, l.package, l.needed, l.per_board, l.short, l.boards_covered
            ));
        }
        if self.is_enough() {
            out.push_str(&format!(
                "ok: the feeders hold enough for {} boards ({} parts on each, {} kinds)\n",
                self.boards,
                self.parts_per_board,
                self.lines.len()
            ));
        } else {
            out.push_str(&format!(
                "short: {} kinds of {} lack {} parts for {} boards; the first reel runs out after {} boards\n",
                self.short_kinds(),
                self.lines.len(),
                self.short_parts(),
                self.boards,
                self.boards_before_reload().unwrap_or(0)
            ));
        }
        out
    }

    /// Every kind as a CSV: `value,package,per_board,available,needed,short,boards_covered,feeders`.
    pub fn csv(&self) -> String {
        let mut out =
            String::from("value,package,per_board,available,needed,short,boards_covered,feeders\n");
        for l in &self.lines {
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                csv_field(&l.value),
                csv_field(&l.package),
                l.per_board,
                l.available,
                l.needed,
                l.short,
                l.boards_covered,
                csv_field(&l.feeders.join(" "))
            ));
        }
        out
    }
}
