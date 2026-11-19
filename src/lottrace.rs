// EH3D-PHOENIX-PNP - which reel, and so which lot, every part of a board came from.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A board that goes to a customer who asks for the trace of its parts needs to say, for each reference, the lot of the reel its part was taken from. The plan says which feeder gives each
//! part; the operator knows which reel is in which feeder. This joins them: a small table `feeder,lot[,reel][,date_code][,supplier]` says what is in each feeder, and the trace lists for each
//! placed reference its feeder and the lot, the reel, the date code and the supplier of it, and for each lot the parts it gave. A feeder that gives parts and has no row, a part that no feeder
//! holds and a row that names a feeder that is not in the table of feeders are told. It reads and writes nothing else; the same board and the same tables give the same trace.

use crate::error::{Code, PnpError};
use crate::feeding::{FeederSpec, Picks};
use crate::parts::{csv_field, detect_delimiter, read_records, Part};
use std::collections::{BTreeMap, BTreeSet};

/// The most rows of a table of lots.
pub const MAX_LOTS: usize = 5000;
/// The most characters of a field of the table.
const MAX_FIELD: usize = 64;

fn bad(message: impl Into<String>) -> PnpError {
    PnpError::new(Code::FeederBadRow, message)
}

/// What is in one feeder.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Lot {
    pub feeder: String,
    pub lot: String,
    pub reel: String,
    pub date_code: String,
    pub supplier: String,
}

fn field_ok(text: &str) -> bool {
    text.chars().count() <= MAX_FIELD && !text.chars().any(char::is_control)
}

/// The table of lots: a header with `feeder` and `lot` (and, if wanted, `reel`, `date_code` and `supplier`) and a row for each feeder; a feeder named twice is refused.
pub fn parse_lots(text: &str) -> Result<Vec<Lot>, PnpError> {
    let text = text.trim_start_matches('\u{feff}');
    let head_line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let records = read_records(text, detect_delimiter(head_line)).map_err(bad)?;
    let Some((header, rows)) = records.split_first() else {
        return Err(bad("the table of lots is empty"));
    };
    let header: Vec<String> = header.iter().map(|h| h.to_ascii_lowercase()).collect();
    let column = |name: &str| header.iter().position(|h| h == name);
    let (Some(feeder_at), Some(lot_at)) = (column("feeder"), column("lot")) else {
        return Err(bad("the table of lots needs the columns feeder and lot"));
    };
    let (reel_at, code_at, supplier_at) = (column("reel"), column("date_code"), column("supplier"));
    if rows.len() > MAX_LOTS {
        return Err(bad(format!(
            "more than {MAX_LOTS} rows in the table of lots"
        )));
    }
    let mut out: Vec<Lot> = Vec::new();
    for (n, row) in rows.iter().enumerate() {
        let get = |at: Option<usize>| at.and_then(|i| row.get(i)).cloned().unwrap_or_default();
        let lot = Lot {
            feeder: get(Some(feeder_at)),
            lot: get(Some(lot_at)),
            reel: get(reel_at),
            date_code: get(code_at),
            supplier: get(supplier_at),
        };
        let line = n + 2;
        if lot.feeder.is_empty() || lot.lot.is_empty() {
            return Err(bad(format!("row {line}: a feeder and a lot are needed")));
        }
        for f in [
            &lot.feeder,
            &lot.lot,
            &lot.reel,
            &lot.date_code,
            &lot.supplier,
        ] {
            if !field_ok(f) {
                return Err(bad(format!(
                    "row {line}: a field is longer than {MAX_FIELD} characters or has a control character"
                )));
            }
        }
        if out.iter().any(|l| l.feeder == lot.feeder) {
            return Err(bad(format!(
                "row {line}: the feeder {} is in the table twice",
                lot.feeder
            )));
        }
        out.push(lot);
    }
    Ok(out)
}

/// One placed part and where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub reference: String,
    pub value: String,
    pub package: String,
    pub feeder: String,
    /// `None` when the feeder has no row in the table of lots.
    pub lot: Option<Lot>,
}

/// The parts that one lot gave.
#[derive(Debug, Clone, PartialEq)]
pub struct LotUse {
    pub lot: Lot,
    pub references: Vec<String>,
}

/// The trace of a board.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Trace {
    /// The placed parts in the order of their references.
    pub rows: Vec<Row>,
    /// The lots that gave parts, in the order of their feeders.
    pub lots: Vec<LotUse>,
    /// The feeders that give parts and have no lot.
    pub without_lot: Vec<String>,
    /// The references of the parts that no feeder holds.
    pub without_feeder: Vec<String>,
    /// The feeders of the table of lots that are not in the table of feeders.
    pub unknown_feeders: Vec<String>,
}

impl Trace {
    /// Whether every part has a feeder and every feeder that gives parts has a lot.
    pub fn is_complete(&self) -> bool {
        self.without_lot.is_empty() && self.without_feeder.is_empty()
    }
}

/// Joins the picks with the table of lots.
pub fn trace(parts: &[Part], feeders: &[FeederSpec], picks: &Picks, lots: &[Lot]) -> Trace {
    let by_feeder: BTreeMap<&str, &Lot> = lots.iter().map(|l| (l.feeder.as_str(), l)).collect();
    let mut rows: Vec<Row> = picks
        .picks
        .iter()
        .map(|p| {
            let part = &parts[p.part];
            let feeder = &feeders[p.feeder].base.id;
            Row {
                reference: part.reference.clone(),
                value: part.value.clone(),
                package: part.package.clone(),
                feeder: feeder.clone(),
                lot: by_feeder.get(feeder.as_str()).map(|l| (*l).clone()),
            }
        })
        .collect();
    rows.sort_by(|a, b| a.reference.cmp(&b.reference));
    let mut per_feeder: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for r in &rows {
        per_feeder
            .entry(r.feeder.as_str())
            .or_default()
            .push(r.reference.clone());
    }
    let mut used = Vec::new();
    let mut without_lot = Vec::new();
    for f in feeders {
        let id = f.base.id.as_str();
        let Some(refs) = per_feeder.get(id) else {
            continue;
        };
        match by_feeder.get(id) {
            Some(l) => used.push(LotUse {
                lot: (*l).clone(),
                references: refs.clone(),
            }),
            None => without_lot.push(id.to_string()),
        }
    }
    let known: BTreeSet<&str> = feeders.iter().map(|f| f.base.id.as_str()).collect();
    let unknown_feeders = lots
        .iter()
        .filter(|l| !known.contains(l.feeder.as_str()))
        .map(|l| l.feeder.clone())
        .collect();
    let mut without_feeder = picks.without_feeder.clone();
    without_feeder.sort();
    Trace {
        rows,
        lots: used,
        without_lot,
        without_feeder,
        unknown_feeders,
    }
}

impl Trace {
    /// The trace for a person: a line for each lot with its parts, then what is missing.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for u in &self.lots {
            let l = &u.lot;
            let mut extra = String::new();
            for (name, v) in [
                ("reel", &l.reel),
                ("date code", &l.date_code),
                ("supplier", &l.supplier),
            ] {
                if !v.is_empty() {
                    extra.push_str(&format!(", {name} {v}"));
                }
            }
            out.push_str(&format!(
                "lot {} (feeder {}{extra}): {} parts: {}\n",
                l.lot,
                l.feeder,
                u.references.len(),
                u.references.join(" ")
            ));
        }
        if !self.without_lot.is_empty() {
            out.push_str(&format!(
                "no lot for the feeders: {}\n",
                self.without_lot.join(", ")
            ));
        }
        if !self.without_feeder.is_empty() {
            out.push_str(&format!(
                "no feeder holds: {}\n",
                self.without_feeder.join(" ")
            ));
        }
        if !self.unknown_feeders.is_empty() {
            out.push_str(&format!(
                "the table of lots names feeders that are not in the table of feeders: {}\n",
                self.unknown_feeders.join(", ")
            ));
        }
        out.push_str(&format!(
            "{} parts traced, {} lots\n",
            self.rows.iter().filter(|r| r.lot.is_some()).count(),
            self.lots.len()
        ));
        out
    }

    /// The trace as a CSV: `board,reference,value,package,feeder,lot,reel,date_code,supplier` (`board` is the serial given, or empty); a part with no lot has the cells empty.
    pub fn csv(&self, board: &str) -> String {
        let mut out =
            String::from("board,reference,value,package,feeder,lot,reel,date_code,supplier\n");
        for r in &self.rows {
            let empty = Lot::default();
            let l = r.lot.as_ref().unwrap_or(&empty);
            let cells = [
                board,
                &r.reference,
                &r.value,
                &r.package,
                &r.feeder,
                &l.lot,
                &l.reel,
                &l.date_code,
                &l.supplier,
            ];
            let line: Vec<String> = cells.iter().map(|c| csv_field(c)).collect();
            out.push_str(&line.join(","));
            out.push('\n');
        }
        out
    }
}
