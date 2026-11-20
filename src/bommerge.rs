// EH3D-PHOENIX-PNP - the bills of materials of several boards, added up into one list to buy.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! An order is rarely one board: a panel of two designs, three boards of a product, a kit and its spare. Each board has its own bill of materials, and the parts to buy are the sum. This adds the
//! rows of several bills, each for the number of boards that are wanted, joining the rows that are the same part (the same value once written alike, the same package, the same part number)
//! and saying which board each part is for, so that the list to buy is made once. The quantity of a row is the number of its references, which is what the board takes; nothing is rounded
//! up for spares (`bom-order` does that).

use crate::bom::{normalize_value, Bom};
use crate::error::{Code, PnpError};
use crate::parts::csv_field;
use std::collections::BTreeMap;

/// The most boards of one bill.
pub const MAX_BOARDS: u64 = 1_000_000;
/// The most bills of one call.
pub const MAX_BILLS: usize = 64;

/// One bill and how many boards are made from it.
#[derive(Debug, Clone, PartialEq)]
pub struct Input {
    pub name: String,
    pub boards: u64,
    pub bom: Bom,
}

/// One part to buy.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub value: String,
    pub package: String,
    pub part_number: String,
    /// The parts for all the boards.
    pub total: u64,
    /// For each bill that has the part: its name, the parts one board takes and the boards.
    pub used_by: Vec<(String, u64, u64)>,
}

fn bad(detail: impl Into<String>) -> PnpError {
    PnpError::new(Code::BomBad, detail.into())
}

/// Adds the bills. Two rows are one part when their value (as `normalize_value` writes it), their package and their part number are the same without regard to case. The list is in the order
/// of the package, then the value, then the part number.
pub fn merge(inputs: &[Input]) -> Result<Vec<Line>, PnpError> {
    if inputs.is_empty() {
        return Err(bad("there is no bill to add"));
    }
    if inputs.len() > MAX_BILLS {
        return Err(bad(format!("up to {MAX_BILLS} bills can be added at once")));
    }
    let mut names = std::collections::BTreeSet::new();
    for i in inputs {
        if !(1..=MAX_BOARDS).contains(&i.boards) {
            return Err(bad(format!(
                "{}: the number of boards is from 1 to {MAX_BOARDS}",
                i.name
            )));
        }
        if !names.insert(i.name.as_str()) {
            return Err(bad(format!("the bill {} is given twice", i.name)));
        }
    }
    type Key = (String, String, String);
    let mut by: BTreeMap<Key, Line> = BTreeMap::new();
    for i in inputs {
        let mut per_board: BTreeMap<Key, (String, String, String, u64)> = BTreeMap::new();
        for l in &i.bom.lines {
            let value = normalize_value(&l.value);
            let key = (
                l.package.trim().to_ascii_lowercase(),
                value.to_ascii_lowercase(),
                l.part_number.trim().to_ascii_lowercase(),
            );
            let e = per_board.entry(key).or_insert_with(|| {
                (
                    l.value.trim().to_string(),
                    l.package.trim().to_string(),
                    l.part_number.trim().to_string(),
                    0,
                )
            });
            e.3 += l.references.len() as u64;
        }
        for (key, (value, package, part_number, n)) in per_board {
            let line = by.entry(key).or_insert_with(|| Line {
                value,
                package,
                part_number,
                total: 0,
                used_by: Vec::new(),
            });
            line.total += n * i.boards;
            line.used_by.push((i.name.clone(), n, i.boards));
        }
    }
    Ok(by.into_values().collect())
}

/// The list as a table: `value,package,part_number,total,used_by` (`used_by` is `name:parts x boards;...`).
pub fn csv(lines: &[Line]) -> String {
    let mut out = String::from("value,package,part_number,total,used_by\n");
    for l in lines {
        let used: Vec<String> = l
            .used_by
            .iter()
            .map(|(name, n, boards)| format!("{name}:{n}x{boards}"))
            .collect();
        out.push_str(&format!(
            "{},{},{},{},{}\n",
            csv_field(&l.value),
            csv_field(&l.package),
            csv_field(&l.part_number),
            l.total,
            csv_field(&used.join(";"))
        ));
    }
    out
}

/// The list for a person: a line that sums it up and a line for each part.
pub fn text(lines: &[Line], inputs: &[Input]) -> String {
    let boards: u64 = inputs.iter().map(|i| i.boards).sum();
    let parts: u64 = lines.iter().map(|l| l.total).sum();
    let mut out = format!(
        "{} bills for {boards} boards: {} different parts, {parts} parts in all\n",
        inputs.len(),
        lines.len()
    );
    for l in lines {
        let who: Vec<String> = l
            .used_by
            .iter()
            .map(|(name, n, b)| format!("{name} {n} x {b}"))
            .collect();
        out.push_str(&format!(
            "{} {}{}: {} ({})\n",
            l.value,
            l.package,
            if l.part_number.is_empty() {
                String::new()
            } else {
                format!(" [{}]", l.part_number)
            },
            l.total,
            who.join(", ")
        ));
    }
    out
}
