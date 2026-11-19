// EH3D-PHOENIX-PNP - what a table of parts holds, and which parts sit too near each other.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! Before a board is planned a person wants to see what it is made of: how many parts, on which side, of which packages and values, how large it is on the machine, and whether two
//! parts were drawn on top of each other (a part copied twice, a library that put two footprints at one origin). This counts the table and finds the pairs of parts of one side whose
//! centres are closer than a distance, without comparing every part with every other: the parts go in a grid whose cells are as wide as the distance.

use crate::error::{Code, PnpError};
use crate::parts::Part;
use std::collections::{BTreeMap, HashMap};

/// The nearest and the farthest a distance of `near` may be, in millimetres.
pub const NEAR_RANGE_MM: (f64, f64) = (0.01, 50.0);

/// How many parts of a package, and how many values they have.
#[derive(Debug, Clone, PartialEq)]
pub struct PackageCount {
    pub package: String,
    pub count: usize,
    pub values: usize,
}

/// What a table of parts holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Stats {
    pub total: usize,
    pub top: usize,
    pub bottom: usize,
    /// The parts that the table marks as not to be mounted.
    pub not_mounted: usize,
    pub values: usize,
    /// By number of parts, the most first; the same number by name.
    pub packages: Vec<PackageCount>,
    /// The corners of the box that holds the centres of the parts, `None` for an empty table.
    pub corners: Option<((f64, f64), (f64, f64))>,
}

/// Counts the parts to be mounted; `not_mounted` is how many more the table left out.
pub fn stats(parts: &[Part], not_mounted: usize) -> Stats {
    let mut packages: BTreeMap<String, (String, usize, BTreeMap<String, ()>)> = BTreeMap::new();
    let mut values: BTreeMap<String, ()> = BTreeMap::new();
    let (mut top, mut bottom) = (0, 0);
    let mut corners: Option<((f64, f64), (f64, f64))> = None;
    for p in parts {
        if p.side == crate::parts::Side::Top {
            top += 1;
        } else {
            bottom += 1;
        }
        let e = packages
            .entry(p.package.trim().to_ascii_lowercase())
            .or_insert_with(|| (p.package.trim().to_string(), 0, BTreeMap::new()));
        e.1 += 1;
        e.2.insert(p.value.trim().to_ascii_lowercase(), ());
        values.insert(p.key(), ());
        corners = Some(match corners {
            None => ((p.x, p.y), (p.x, p.y)),
            Some((lo, hi)) => (
                (lo.0.min(p.x), lo.1.min(p.y)),
                (hi.0.max(p.x), hi.1.max(p.y)),
            ),
        });
    }
    let mut list: Vec<PackageCount> = packages
        .into_values()
        .map(|(package, count, v)| PackageCount {
            package,
            count,
            values: v.len(),
        })
        .collect();
    list.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.package.to_lowercase().cmp(&b.package.to_lowercase()))
    });
    Stats {
        total: parts.len(),
        top,
        bottom,
        not_mounted,
        values: values.len(),
        packages: list,
        corners,
    }
}

impl Stats {
    /// The size of the box that holds the centres of the parts, in millimetres.
    pub fn size(&self) -> Option<(f64, f64)> {
        self.corners.map(|(lo, hi)| (hi.0 - lo.0, hi.1 - lo.1))
    }

    /// The table in words: the sides, the box, the kinds and a line for each package.
    pub fn text(&self) -> String {
        let mut out = format!(
            "{} parts ({} on the top, {} on the bottom), {} not to be mounted, {} kinds of part (value and package)\n",
            self.total, self.top, self.bottom, self.not_mounted, self.values
        );
        if let (Some((lo, hi)), Some((w, h))) = (self.corners, self.size()) {
            out.push_str(&format!(
                "centres from {:.3},{:.3} to {:.3},{:.3}: {:.3} x {:.3} mm\n",
                lo.0, lo.1, hi.0, hi.1, w, h
            ));
        }
        for p in &self.packages {
            out.push_str(&format!(
                "package {}: {} parts, {} values\n",
                p.package, p.count, p.values
            ));
        }
        out
    }
}

/// Two parts of one side whose centres are closer than the distance asked.
#[derive(Debug, Clone, PartialEq)]
pub struct Near {
    pub first: String,
    pub second: String,
    pub dist_mm: f64,
}

/// The pairs of parts of one side whose centres are less than `within_mm` apart, the nearest first (the same distance by the references). Parts that are on top of each other (the same
/// place) are the nearest of all. The distance has to be from 0.01 to 50 mm.
pub fn near(parts: &[Part], within_mm: f64) -> Result<Vec<Near>, PnpError> {
    if !(within_mm.is_finite() && (NEAR_RANGE_MM.0..=NEAR_RANGE_MM.1).contains(&within_mm)) {
        return Err(PnpError::new(
            Code::PlanSetting,
            format!(
                "the distance must be from {} to {} mm",
                NEAR_RANGE_MM.0, NEAR_RANGE_MM.1
            ),
        ));
    }
    let cell = |v: f64| (v / within_mm).floor() as i64;
    let mut grid: HashMap<(i64, i64, bool), Vec<usize>> = HashMap::new();
    for (i, p) in parts.iter().enumerate() {
        if p.x.is_finite() && p.y.is_finite() {
            grid.entry((cell(p.x), cell(p.y), p.side == crate::parts::Side::Top))
                .or_default()
                .push(i);
        }
    }
    let mut found: Vec<Near> = Vec::new();
    for (&(cx, cy, top), members) in &grid {
        for &i in members {
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let Some(others) = grid.get(&(cx + dx, cy + dy, top)) else {
                        continue;
                    };
                    for &j in others {
                        // each pair once
                        if j <= i {
                            continue;
                        }
                        let d = (parts[i].x - parts[j].x).hypot(parts[i].y - parts[j].y);
                        if d < within_mm {
                            let (a, b) = (&parts[i].reference, &parts[j].reference);
                            let (a, b) = if a <= b { (a, b) } else { (b, a) };
                            found.push(Near {
                                first: a.clone(),
                                second: b.clone(),
                                dist_mm: d,
                            });
                        }
                    }
                }
            }
        }
    }
    found.sort_by(|a, b| {
        a.dist_mm
            .total_cmp(&b.dist_mm)
            .then_with(|| a.first.cmp(&b.first))
            .then_with(|| a.second.cmp(&b.second))
    });
    Ok(found)
}

/// The pairs as lines, at most `limit` of them, then a line that says how many there were.
pub fn near_text(pairs: &[Near], within_mm: f64, limit: usize) -> String {
    let mut out = String::new();
    for n in pairs.iter().take(limit) {
        out.push_str(&format!(
            "{} and {} are {:.3} mm apart\n",
            n.first, n.second, n.dist_mm
        ));
    }
    if pairs.len() > limit {
        out.push_str(&format!("... and {} more\n", pairs.len() - limit));
    }
    out.push_str(&format!(
        "{} pairs of parts closer than {within_mm} mm\n",
        pairs.len()
    ));
    out
}
