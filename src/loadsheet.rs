//! The load sheet: which kind of part goes in which slot of the machine. The kinds that most parts of the board need are given the free slots nearest to the middle of the board, so the
//! head has the shortest way to and from the feeders; a tape only goes in a slot that is wide enough for it. The sheet can be printed for the operator or read back as the file of
//! feeders that the planner uses.

use crate::catalog::Catalog;
use crate::config::MachineConfig;
use crate::error::{Code, PnpError};
use crate::parts::{detect_delimiter, read_records, Part, Side};
use std::collections::BTreeMap;

pub const MAX_SLOTS: usize = 10_000;

/// A place of the machine where a tape can be put.
#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    pub id: String,
    /// Where the part is picked from a tape in this slot, in millimetres.
    pub x: f64,
    pub y: f64,
    /// The widest tape the slot takes.
    pub max_width_mm: f64,
}

/// A CSV of slots with the header `id,x,y,max_width_mm`.
pub fn slots_from_csv(text: &str) -> Result<Vec<Slot>, PnpError> {
    let bad = |m: String| PnpError::new(Code::SlotsBad, m);
    let text = text.trim_start_matches('\u{feff}');
    let head_line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .ok_or_else(|| bad("the file of slots is empty".to_string()))?;
    let mut records = read_records(text, detect_delimiter(head_line))
        .map_err(&bad)?
        .into_iter();
    let head: Vec<String> = records
        .next()
        .ok_or_else(|| bad("the file of slots is empty".to_string()))?
        .iter()
        .map(|h| h.to_ascii_lowercase())
        .collect();
    if head != ["id", "x", "y", "max_width_mm"] {
        return Err(bad(
            "the first row of the slots must be exactly: id,x,y,max_width_mm".to_string(),
        ));
    }
    let mut out: Vec<Slot> = Vec::new();
    for (i, f) in records.enumerate() {
        let row = i + 2;
        if out.len() >= MAX_SLOTS {
            return Err(bad(format!("more than {MAX_SLOTS} slots")));
        }
        if f.len() != 4 {
            return Err(bad(format!("slot row {row}: {} fields, not 4", f.len())));
        }
        let num = |s: &str, lo: f64, hi: f64| {
            s.parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && (lo..=hi).contains(v))
                .ok_or_else(|| {
                    bad(format!(
                        "slot row {row}: '{s}' is not a number from {lo} to {hi}"
                    ))
                })
        };
        if f[0].is_empty() || out.iter().any(|s| s.id == f[0]) {
            return Err(bad(format!(
                "slot row {row}: the id '{}' is empty or used twice",
                f[0]
            )));
        }
        out.push(Slot {
            id: f[0].clone(),
            x: num(&f[1], -10_000.0, 10_000.0)?,
            y: num(&f[2], -10_000.0, 10_000.0)?,
            max_width_mm: num(&f[3], 1.0, 100.0)?,
        });
    }
    Ok(out)
}

/// A kind of part the board needs, and how many.
#[derive(Debug, Clone, PartialEq)]
pub struct Need {
    pub value: String,
    pub package: String,
    pub count: usize,
    pub tape_mm: f64,
    pub tip: String,
}

/// The kinds of part on `side` of the board (all the parts for `None`) with how many of each, from the kind that is used most down, and the middle of those parts.
pub fn needs(
    parts: &[Part],
    side: Option<Side>,
    catalog: &Catalog,
    cfg: &MachineConfig,
) -> (Vec<Need>, (f64, f64)) {
    let defaults = cfg.part_defaults();
    let mut kinds: BTreeMap<String, Need> = BTreeMap::new();
    let (mut sx, mut sy, mut n) = (0.0, 0.0, 0usize);
    for p in parts.iter().filter(|p| !side.is_some_and(|s| s != p.side)) {
        sx += p.x;
        sy += p.y;
        n += 1;
        let props = catalog.resolve(p, &defaults);
        kinds
            .entry(p.key())
            .and_modify(|k| k.count += 1)
            .or_insert(Need {
                value: p.value.trim().to_string(),
                package: p.package.trim().to_string(),
                count: 1,
                tape_mm: props.tape_mm,
                tip: props.nozzle_tip,
            });
    }
    let mut list: Vec<Need> = kinds.into_values().collect();
    list.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.value.cmp(&b.value))
            .then_with(|| a.package.cmp(&b.package))
    });
    let middle = if n == 0 {
        (0.0, 0.0)
    } else {
        (sx / n as f64, sy / n as f64)
    };
    (list, middle)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub slot: Slot,
    pub need: Need,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LoadSheet {
    /// The kinds that have a slot, in the order of the slots by id.
    pub rows: Vec<Row>,
    /// The kinds for which no free slot is wide enough.
    pub unassigned: Vec<Need>,
}

/// Gives each kind of part the free slot that is nearest to `middle` among those wide enough for its tape (of two equally near, the lower id). The widest tapes are placed first, and
/// among tapes of the same width the kind that most parts need.
pub fn assign_slots(needs: &[Need], slots: &[Slot], middle: (f64, f64)) -> LoadSheet {
    let mut order: Vec<usize> = (0..slots.len()).collect();
    order.sort_by(|&a, &b| {
        let da = (slots[a].x - middle.0).hypot(slots[a].y - middle.1);
        let db = (slots[b].x - middle.0).hypot(slots[b].y - middle.1);
        da.total_cmp(&db)
            .then_with(|| slots[a].id.cmp(&slots[b].id))
    });
    let mut used = vec![false; slots.len()];
    let mut sheet = LoadSheet::default();
    // the widest tapes first, because they fit in the fewest slots; among tapes of the same width the kind that most parts need goes first
    let mut by_width: Vec<&Need> = needs.iter().collect();
    by_width.sort_by(|a, b| {
        b.tape_mm
            .total_cmp(&a.tape_mm)
            .then_with(|| b.count.cmp(&a.count))
    });
    for need in by_width {
        match order
            .iter()
            .find(|&&s| !used[s] && slots[s].max_width_mm >= need.tape_mm)
        {
            Some(&s) => {
                used[s] = true;
                sheet.rows.push(Row {
                    slot: slots[s].clone(),
                    need: need.clone(),
                });
            }
            None => sheet.unassigned.push(need.clone()),
        }
    }
    sheet.rows.sort_by(|a, b| a.slot.id.cmp(&b.slot.id));
    sheet
}

fn field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r', ';']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

impl LoadSheet {
    /// The sheet for the operator: one line per slot.
    pub fn to_csv(&self) -> String {
        let mut out = String::from("slot,value,package,count,tape_mm,tip,x,y\n");
        for r in &self.rows {
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                field(&r.slot.id),
                field(&r.need.value),
                field(&r.need.package),
                r.need.count,
                r.need.tape_mm,
                field(&r.need.tip),
                r.slot.x,
                r.slot.y
            ));
        }
        out
    }

    /// The same sheet as the file of feeders that the planner reads: one tape feeder per slot, holding as many parts as the board needs.
    pub fn to_feeders_csv(&self) -> String {
        let mut out = String::from("id,value,package,x,y,count\n");
        for r in &self.rows {
            out.push_str(&format!(
                "{},{},{},{},{},{}\n",
                field(&r.slot.id),
                field(&r.need.value),
                field(&r.need.package),
                r.slot.x,
                r.slot.y,
                r.need.count
            ));
        }
        out
    }
}
