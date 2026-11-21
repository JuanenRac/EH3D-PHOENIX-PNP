// EH3D-PHOENIX-PNP - the composite order of the places: a list of keys, each ascending or descending.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The setting `order` has one answer (nearest, part, height_part, pick, tips or none). A person who builds a panel of several boards, or who wants the tall parts last and, among the parts
//! of one height, the ones that are the most, wants a list: first this, then that, and each one up or down. `placement_order` is that list, `unit:asc, height:asc, quantity:desc`: the
//! places are put in groups by the first key, the groups in the order of its direction, each group in groups by the second key, and so on; inside the last group the head goes to the nearest
//! place that is left, as it does for any order. A key says one thing about a place:
//!
//! * `unit`: the board of a panel the place belongs to (what comes before the `/` of its reference, `A` in `A/R1`; a place of a single board has none, and all its places are one unit);
//! * `feeder`: the identifier of the feeder it is picked from;
//! * `part`: its kind (value and package);
//! * `height`: the height of its package;
//! * `area`: the area of its body, from the table of packages (a place whose body is not known has area 0);
//! * `quantity`: how many places of the same kind there are in the same unit.
//!
//! Text keys compare as text, the others as numbers, and a tie is left to the next key. The list holds at most 6 keys, each once. The cost of a part is not a key: the prices are in their own
//! table and the planner does not read it.

use std::cmp::Ordering;

/// The most keys of the list.
pub const MAX_TERMS: usize = 6;

/// What a key says about a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Unit,
    Feeder,
    Part,
    Height,
    Area,
    Quantity,
}

impl Key {
    pub const ALL: [Key; 6] = [
        Key::Unit,
        Key::Feeder,
        Key::Part,
        Key::Height,
        Key::Area,
        Key::Quantity,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Key::Unit => "unit",
            Key::Feeder => "feeder",
            Key::Part => "part",
            Key::Height => "height",
            Key::Area => "area",
            Key::Quantity => "quantity",
        }
    }

    pub fn parse(text: &str) -> Option<Key> {
        Key::ALL.into_iter().find(|k| k.name() == text)
    }
}

/// One key with its direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Term {
    pub key: Key,
    pub descending: bool,
}

/// What the keys read of a place.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub unit: String,
    pub feeder: String,
    pub part: String,
    pub height: f64,
    pub area: f64,
    pub quantity: usize,
}

/// The board of a panel that a reference belongs to: what comes before the first `/`, or nothing.
pub fn unit_of(reference: &str) -> String {
    reference
        .split_once('/')
        .map(|(unit, _)| unit.to_string())
        .unwrap_or_default()
}

/// Reads the list: keys separated by commas, each with `:asc` or `:desc` (ascending when it says nothing). An empty text is the empty list, which turns the composite order off.
pub fn parse(text: &str) -> Result<Vec<Term>, String> {
    let mut out: Vec<Term> = Vec::new();
    for raw in text.split(',') {
        let item = raw.trim().to_ascii_lowercase();
        if item.is_empty() {
            continue;
        }
        let (name, direction) = match item.split_once(':') {
            Some((n, d)) => (n.trim(), d.trim()),
            None => (item.as_str(), "asc"),
        };
        let key = Key::parse(name).ok_or_else(|| {
            format!("'{name}' is not a key (one of unit, feeder, part, height, area, quantity)")
        })?;
        let descending = match direction {
            "asc" => false,
            "desc" => true,
            other => return Err(format!("'{other}' is not asc or desc")),
        };
        if out.iter().any(|t| t.key == key) {
            return Err(format!("'{name}' is in the list twice"));
        }
        if out.len() >= MAX_TERMS {
            return Err(format!("more than {MAX_TERMS} keys"));
        }
        out.push(Term { key, descending });
    }
    Ok(out)
}

/// The list as text, as `parse` reads it.
pub fn text(terms: &[Term]) -> String {
    terms
        .iter()
        .map(|t| {
            format!(
                "{}:{}",
                t.key.name(),
                if t.descending { "desc" } else { "asc" }
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn number(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}

/// How two places stand by the list: the first key that tells them apart decides, in its direction.
pub fn compare(terms: &[Term], a: &Row, b: &Row) -> Ordering {
    for t in terms {
        let o = match t.key {
            Key::Unit => a.unit.cmp(&b.unit),
            Key::Feeder => a.feeder.cmp(&b.feeder),
            Key::Part => a.part.cmp(&b.part),
            Key::Height => number(a.height, b.height),
            Key::Area => number(a.area, b.area),
            Key::Quantity => a.quantity.cmp(&b.quantity),
        };
        let o = if t.descending { o.reverse() } else { o };
        if o != Ordering::Equal {
            return o;
        }
    }
    Ordering::Equal
}

/// Puts the indexes in groups of places that the list does not tell apart, in the order the list gives; inside a group the indexes keep their order.
pub fn groups(terms: &[Term], idx: &[usize], row: &dyn Fn(usize) -> Row) -> Vec<Vec<usize>> {
    let rows: Vec<(usize, Row)> = idx.iter().map(|&k| (k, row(k))).collect();
    let mut order: Vec<usize> = (0..rows.len()).collect();
    order.sort_by(|&a, &b| compare(terms, &rows[a].1, &rows[b].1).then(a.cmp(&b)));
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut last: Option<usize> = None;
    for o in order {
        let same = last.is_some_and(|l| compare(terms, &rows[l].1, &rows[o].1) == Ordering::Equal);
        if same {
            if let Some(g) = out.last_mut() {
                g.push(rows[o].0);
            }
        } else {
            out.push(vec![rows[o].0]);
        }
        last = Some(o);
    }
    out
}
