// EH3D-PHOENIX-PNP - how many boards each reel of the feeders lasts.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! `stock` answers for a number of boards that is asked: do the feeders hold enough. A person who starts a day's work asks the other way round: how many boards can I make before a reel
//! runs out, and which reel is it. This takes what the plan takes from each feeder for one board and what the feeder holds, and says for each switched-on feeder how many whole boards it lasts
//! and how many parts are left over after them, the reel that runs out first marked. It reads the plan and the table of feeders and changes nothing. A feeder that no part of the plan takes
//! from lasts for ever and is listed last.

use crate::feeding::FeederSpec;
use crate::parts::csv_field;
use std::collections::BTreeMap;

/// One feeder.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: String,
    pub value: String,
    pub package: String,
    /// The parts in the feeder.
    pub count: u32,
    /// The parts that one board takes from it (0 when the plan does not use it).
    pub per_board: u32,
    /// The whole boards it lasts; `None` when the plan does not use it.
    pub boards: Option<u64>,
    /// The parts that are left after those boards.
    pub left_over: u64,
}

/// The feeders that are switched on, the ones that run out first at the top (the fewest boards, then the id); the ones the plan does not use come last.
pub fn runway(placement_feeders: &[&str], feeders: &[FeederSpec]) -> Vec<Row> {
    let mut takes: BTreeMap<&str, u32> = BTreeMap::new();
    for id in placement_feeders {
        let n = takes.entry(*id).or_insert(0);
        *n = n.saturating_add(1);
    }
    let mut rows: Vec<Row> = feeders
        .iter()
        .filter(|f| f.enabled)
        .map(|f| {
            let per_board = takes.get(f.base.id.as_str()).copied().unwrap_or(0);
            let count = f.base.count;
            // a feeder that no part takes from lasts for ever and keeps all its parts
            let boards = count.checked_div(per_board).map(u64::from);
            let left_over = u64::from(count.checked_rem(per_board).unwrap_or(count));
            Row {
                id: f.base.id.clone(),
                value: f.base.value.trim().to_string(),
                package: f.base.package.trim().to_string(),
                count,
                per_board,
                boards,
                left_over,
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        let key = |r: &Row| (r.boards.is_none(), r.boards.unwrap_or(0));
        key(a).cmp(&key(b)).then_with(|| a.id.cmp(&b.id))
    });
    rows
}

/// The boards that can be made before a reel runs out: the least of the feeders that the plan uses; `None` when it uses none.
pub fn boards_possible(rows: &[Row]) -> Option<u64> {
    rows.iter().filter_map(|r| r.boards).min()
}

/// The feeders that last fewer boards than `wanted`.
pub fn short_of(rows: &[Row], wanted: u64) -> Vec<&Row> {
    rows.iter()
        .filter(|r| r.boards.is_some_and(|b| b < wanted))
        .collect()
}

/// The table for a person: a line for each feeder and the line that says how many boards can be made.
pub fn text(rows: &[Row], wanted: u64) -> String {
    let mut out = String::new();
    for r in rows {
        match r.boards {
            Some(b) => out.push_str(&format!(
                "{}: {} / {}, {} parts, {} for a board, lasts {} boards, {} parts over{}\n",
                r.id,
                r.value,
                r.package,
                r.count,
                r.per_board,
                b,
                r.left_over,
                if b < wanted { "  <- short" } else { "" }
            )),
            None => out.push_str(&format!(
                "{}: {} / {}, {} parts, not used by this board\n",
                r.id, r.value, r.package, r.count
            )),
        }
    }
    match boards_possible(rows) {
        Some(n) => {
            let first = rows.iter().find(|r| r.boards == Some(n));
            out.push_str(&format!(
                "{} boards can be made before a reel runs out: {} runs out first\n",
                n,
                first.map_or("-", |r| r.id.as_str())
            ));
            let short = short_of(rows, wanted).len();
            if short > 0 {
                out.push_str(&format!(
                    "short: {short} feeders last fewer than the {wanted} boards asked for\n"
                ));
            }
        }
        None => out.push_str("the plan takes nothing from the feeders that are switched on\n"),
    }
    out
}

/// The feeders as a CSV: `feeder,value,package,count,per_board,boards,left_over` (`boards` is empty for a feeder that is not used).
pub fn csv(rows: &[Row]) -> String {
    let mut out = String::from("feeder,value,package,count,per_board,boards,left_over\n");
    for r in rows {
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            csv_field(&r.id),
            csv_field(&r.value),
            csv_field(&r.package),
            r.count,
            r.per_board,
            r.boards.map_or(String::new(), |b| b.to_string()),
            r.left_over
        ));
    }
    out
}
