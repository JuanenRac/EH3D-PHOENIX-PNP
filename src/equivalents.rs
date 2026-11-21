// EH3D-PHOENIX-PNP - parts that another part already on a feeder can stand for.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! Every reel that is loaded costs time, so when a board needs a part that no feeder holds and an equivalent one is already on a feeder, the equivalent saves a change. A table
//! `group,value,package` says which parts may stand for each other (the rows with the same group); this goes through the parts of a board, leaves the ones that have a feeder of their
//! own where they are, and for each of the others looks for a feeder that holds an equivalent part and still has parts left after the ones that need it for themselves. The one with the most
//! left is taken (the lowest id when they tie), so the same files always give the same answer. It tells which parts changed, how many reels were not needed, and which parts still have no
//! feeder. It reads tables and writes a new table of parts; it decides nothing that the person has not asked for, and a part is never changed in a file that exists.

use crate::feeders::Feeder;
use crate::parts::{csv_field, detect_delimiter, read_records_numbered, Part};
use std::collections::{BTreeMap, BTreeSet};

/// The most rows of a table of groups.
pub const MAX_ROWS: usize = 20_000;

/// The groups: the key of a part (value and package, without regard to case) and the group it is in.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Groups {
    of: BTreeMap<String, String>,
}

fn key(value: &str, package: &str) -> String {
    format!(
        "{}|{}",
        value.trim().to_ascii_lowercase(),
        package.trim().to_ascii_lowercase()
    )
}

impl Groups {
    /// Reads the table `group,value,package`. A part that is in two groups is an error: the answer would depend on which was read first.
    pub fn parse(text: &str) -> Result<Groups, String> {
        let text = text.trim_start_matches('\u{feff}');
        let head = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        let records = read_records_numbered(text, detect_delimiter(head))
            .map_err(|(line, e)| format!("line {line}: {e}"))?;
        let mut rows = records.into_iter();
        let (head_line, header) = rows.next().ok_or("the table of groups is empty")?;
        let header: Vec<String> = header
            .iter()
            .map(|h| h.trim().to_ascii_lowercase())
            .collect();
        if header != ["group", "value", "package"] {
            return Err(format!(
                "line {head_line}: the first row must be exactly: group,value,package"
            ));
        }
        let mut of: BTreeMap<String, String> = BTreeMap::new();
        for (n, (line, row)) in rows.enumerate() {
            if row.iter().all(|c| c.trim().is_empty()) {
                continue;
            }
            if n >= MAX_ROWS {
                return Err(format!("more than {MAX_ROWS} rows"));
            }
            if row.len() != 3 {
                return Err(format!("line {line}: {} fields, not 3", row.len()));
            }
            let group = row[0].trim().to_string();
            if group.is_empty() || row[1].trim().is_empty() {
                return Err(format!("line {line}: the group and the value are needed"));
            }
            let k = key(&row[1], &row[2]);
            match of.get(&k) {
                Some(g) if *g != group => {
                    return Err(format!(
                        "line {line}: '{} {}' is in the group '{g}' and in '{group}'",
                        row[1].trim(),
                        row[2].trim()
                    ))
                }
                _ => {
                    of.insert(k, group);
                }
            }
        }
        if of.is_empty() {
            return Err("the table of groups has no row".into());
        }
        Ok(Groups { of })
    }

    /// The group of a part, if it is in one.
    pub fn group_of(&self, value: &str, package: &str) -> Option<&str> {
        self.of.get(&key(value, package)).map(String::as_str)
    }

    /// How many parts the table names.
    pub fn len(&self) -> usize {
        self.of.len()
    }

    pub fn is_empty(&self) -> bool {
        self.of.is_empty()
    }
}

/// A part that goes on another feeder than its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Swap {
    pub reference: String,
    pub value: String,
    pub package: String,
    pub feeder: String,
    pub feeder_value: String,
    pub feeder_package: String,
}

/// What was found for a table of parts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outcome {
    /// Parts that a feeder of their own holds.
    pub own: usize,
    pub swaps: Vec<Swap>,
    /// The references that no feeder, own or equivalent, can serve.
    pub missing: Vec<String>,
}

impl Outcome {
    /// How many different kinds of part did not need a reel of their own.
    pub fn reels_saved(&self) -> usize {
        self.swaps
            .iter()
            .map(|s| key(&s.value, &s.package))
            .collect::<BTreeSet<_>>()
            .len()
    }
}

/// Goes through the parts in the order they are given.
pub fn resolve(parts: &[Part], feeders: &[Feeder], groups: &Groups) -> Outcome {
    // what each feeder has left after the parts that need it for themselves
    let mut left: BTreeMap<&str, u64> = feeders
        .iter()
        .map(|f| (f.id.as_str(), u64::from(f.count)))
        .collect();
    let mut out = Outcome::default();
    let mut orphans: Vec<&Part> = Vec::new();
    for p in parts {
        let own: Vec<&Feeder> = feeders.iter().filter(|f| f.key() == p.key()).collect();
        if own.is_empty() {
            orphans.push(p);
            continue;
        }
        out.own += 1;
        // the first of its feeders that still holds a part pays for it; the others are for the strip that runs out
        if let Some(f) = own.iter().find(|f| left[f.id.as_str()] > 0) {
            if let Some(v) = left.get_mut(f.id.as_str()) {
                *v -= 1;
            }
        }
    }
    for p in orphans {
        let Some(group) = groups.group_of(&p.value, &p.package) else {
            out.missing.push(p.reference.clone());
            continue;
        };
        let best = feeders
            .iter()
            .filter(|f| groups.group_of(&f.value, &f.package) == Some(group))
            .filter(|f| left[f.id.as_str()] > 0)
            .max_by(|a, b| {
                left[a.id.as_str()]
                    .cmp(&left[b.id.as_str()])
                    .then_with(|| b.id.cmp(&a.id))
            });
        match best {
            Some(f) => {
                if let Some(v) = left.get_mut(f.id.as_str()) {
                    *v -= 1;
                }
                out.swaps.push(Swap {
                    reference: p.reference.clone(),
                    value: p.value.clone(),
                    package: p.package.clone(),
                    feeder: f.id.clone(),
                    feeder_value: f.value.clone(),
                    feeder_package: f.package.clone(),
                });
            }
            None => out.missing.push(p.reference.clone()),
        }
    }
    out
}

/// The table of parts with the swapped parts given the value and the package of the feeder that serves them, as `reference,value,package,x,y,rotation,side` (the columns that the reader of positions knows).
pub fn parts_csv(parts: &[Part], outcome: &Outcome) -> String {
    let swapped: BTreeMap<&str, &Swap> = outcome
        .swaps
        .iter()
        .map(|s| (s.reference.as_str(), s))
        .collect();
    let mut out = String::from("reference,value,package,x,y,rotation,side\n");
    for p in parts {
        let (value, package) = match swapped.get(p.reference.as_str()) {
            Some(s) => (s.feeder_value.as_str(), s.feeder_package.as_str()),
            None => (p.value.as_str(), p.package.as_str()),
        };
        out.push_str(&format!(
            "{},{},{},{:.3},{:.3},{:.1},{}\n",
            csv_field(&p.reference),
            csv_field(value),
            csv_field(package),
            p.x,
            p.y,
            p.rotation,
            if p.side == crate::parts::Side::Bottom {
                "bottom"
            } else {
                "top"
            }
        ));
    }
    out
}
