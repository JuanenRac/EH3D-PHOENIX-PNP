// EH3D-PHOENIX-PNP - what changed between two tables of parts.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A board is redrawn, and the position file that comes out of the CAD program is not the one the line was set up with. Before the feeders are touched a person wants to know what is
//! different: which parts were added or taken away, which moved or turned, which got another value, another package or the other side. This compares two tables of parts by their
//! references and says it part by part, with a tolerance for the noise of the third decimal and for angles that are the same turn written another way.

use crate::error::{Code, PnpError};
use crate::parts::{csv_field, Part, Side};
use std::collections::BTreeMap;

/// The largest tolerance of the places, in millimetres.
pub const MAX_TOL_MM: f64 = 100.0;

/// How near two places or two turns have to be to be the same.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub tol_mm: f64,
    pub rot_tol_deg: f64,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            tol_mm: 0.01,
            rot_tol_deg: 0.5,
        }
    }
}

impl Settings {
    /// The tolerances as numbers in range.
    pub fn check(&self) -> Result<(), PnpError> {
        if !(self.tol_mm.is_finite() && (0.0..=MAX_TOL_MM).contains(&self.tol_mm)) {
            return Err(PnpError::new(
                Code::PlanSetting,
                format!("the tolerance of the places must be from 0 to {MAX_TOL_MM} mm"),
            ));
        }
        if !(self.rot_tol_deg.is_finite() && (0.0..=180.0).contains(&self.rot_tol_deg)) {
            return Err(PnpError::new(
                Code::PlanSetting,
                "the tolerance of the turns must be from 0 to 180 degrees",
            ));
        }
        Ok(())
    }
}

/// One difference of one part.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub reference: String,
    /// `added`, `removed`, `moved`, `turned`, `value`, `package` or `side`.
    pub kind: &'static str,
    /// How the part was (empty for an added part) and how it is (empty for a removed one).
    pub old: String,
    pub new: String,
}

/// The names of the kinds of change, in the order they are counted.
pub const KINDS: [&str; 7] = [
    "added", "removed", "moved", "turned", "value", "package", "side",
];

/// What two tables of parts have of the same and of different.
#[derive(Debug, Clone, PartialEq)]
pub struct Diff {
    pub old_parts: usize,
    pub new_parts: usize,
    /// The parts that are in both and did not change.
    pub same: usize,
    /// By reference, and for a part in the order of `KINDS`.
    pub changes: Vec<Change>,
}

fn side_name(s: Side) -> &'static str {
    match s {
        Side::Top => "top",
        Side::Bottom => "bottom",
    }
}

fn turn(deg: f64) -> f64 {
    let r = deg.rem_euclid(360.0);
    if r > 359.999_999 {
        0.0
    } else {
        r
    }
}

/// The smallest angle between two turns, in degrees.
fn turn_between(a: f64, b: f64) -> f64 {
    let d = (turn(a) - turn(b)).abs();
    d.min(360.0 - d)
}

fn describe(p: &Part) -> String {
    format!(
        "{} {} at {:.3},{:.3} turned {:.1} on the {}",
        p.value.trim(),
        p.package.trim(),
        p.x,
        p.y,
        turn(p.rotation),
        side_name(p.side)
    )
}

fn by_reference<'a>(
    parts: &'a [Part],
    which: &str,
) -> Result<BTreeMap<&'a str, &'a Part>, PnpError> {
    let mut map = BTreeMap::new();
    for p in parts {
        let r = p.reference.trim();
        if map.insert(r, p).is_some() {
            return Err(PnpError::new(
                Code::PartsBad,
                format!("the reference {r} is twice in the {which} table"),
            ));
        }
    }
    Ok(map)
}

/// Compares two tables of parts by their references. A reference that is twice in one table is an error (the comparison would be a guess). A value or a package that differs only in
/// case is the same; a place within `tol_mm` and a turn within `rot_tol_deg` are the same.
pub fn diff(old: &[Part], new: &[Part], settings: &Settings) -> Result<Diff, PnpError> {
    settings.check()?;
    let a = by_reference(old, "first")?;
    let b = by_reference(new, "second")?;
    let mut changes = Vec::new();
    let mut same = 0usize;
    let mut refs: Vec<&str> = a.keys().chain(b.keys()).copied().collect();
    refs.sort_unstable();
    refs.dedup();
    for r in refs {
        match (a.get(r), b.get(r)) {
            (Some(p), None) => changes.push(Change {
                reference: r.to_string(),
                kind: "removed",
                old: describe(p),
                new: String::new(),
            }),
            (None, Some(q)) => changes.push(Change {
                reference: r.to_string(),
                kind: "added",
                old: String::new(),
                new: describe(q),
            }),
            (Some(p), Some(q)) => {
                let before = changes.len();
                let dist = (p.x - q.x).hypot(p.y - q.y);
                if dist > settings.tol_mm {
                    changes.push(Change {
                        reference: r.to_string(),
                        kind: "moved",
                        old: format!("{:.3},{:.3}", p.x, p.y),
                        new: format!("{:.3},{:.3}", q.x, q.y),
                    });
                }
                if turn_between(p.rotation, q.rotation) > settings.rot_tol_deg {
                    changes.push(Change {
                        reference: r.to_string(),
                        kind: "turned",
                        old: format!("{:.1}", turn(p.rotation)),
                        new: format!("{:.1}", turn(q.rotation)),
                    });
                }
                if !p.value.trim().eq_ignore_ascii_case(q.value.trim()) {
                    changes.push(Change {
                        reference: r.to_string(),
                        kind: "value",
                        old: p.value.trim().to_string(),
                        new: q.value.trim().to_string(),
                    });
                }
                if !p.package.trim().eq_ignore_ascii_case(q.package.trim()) {
                    changes.push(Change {
                        reference: r.to_string(),
                        kind: "package",
                        old: p.package.trim().to_string(),
                        new: q.package.trim().to_string(),
                    });
                }
                if p.side != q.side {
                    changes.push(Change {
                        reference: r.to_string(),
                        kind: "side",
                        old: side_name(p.side).to_string(),
                        new: side_name(q.side).to_string(),
                    });
                }
                if changes.len() == before {
                    same += 1;
                }
            }
            (None, None) => {}
        }
    }
    Ok(Diff {
        old_parts: old.len(),
        new_parts: new.len(),
        same,
        changes,
    })
}

impl Diff {
    /// How many changes of a kind of `KINDS` there are.
    pub fn count(&self, kind: &str) -> usize {
        self.changes.iter().filter(|c| c.kind == kind).count()
    }

    /// Whether the two tables are the same part for part.
    pub fn is_same(&self) -> bool {
        self.changes.is_empty()
    }

    /// The parts that have at least one change.
    pub fn parts_changed(&self) -> usize {
        let mut refs: Vec<&str> = self.changes.iter().map(|c| c.reference.as_str()).collect();
        refs.dedup();
        refs.len()
    }

    /// The line that sums it up.
    pub fn summary(&self) -> String {
        let kinds: Vec<String> = KINDS
            .iter()
            .filter(|k| self.count(k) > 0)
            .map(|k| format!("{} {k}", self.count(k)))
            .collect();
        format!(
            "{} parts in the first table, {} in the second, {} the same{}",
            self.old_parts,
            self.new_parts,
            self.same,
            if kinds.is_empty() {
                String::new()
            } else {
                format!(": {}", kinds.join(", "))
            }
        )
    }

    /// The report, one line for each change, then the summary.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for c in &self.changes {
            out.push_str(&match c.kind {
                "added" => format!("{}: added: {}\n", c.reference, c.new),
                "removed" => format!("{}: removed: {}\n", c.reference, c.old),
                k => format!("{}: {k}: {} -> {}\n", c.reference, c.old, c.new),
            });
        }
        out.push_str(&self.summary());
        out.push('\n');
        out
    }

    /// The changes as a CSV: `reference,change,old,new`.
    pub fn csv(&self) -> String {
        let mut out = String::from("reference,change,old,new\n");
        for c in &self.changes {
            out.push_str(&format!(
                "{},{},{},{}\n",
                csv_field(&c.reference),
                c.kind,
                csv_field(&c.old),
                csv_field(&c.new)
            ));
        }
        out
    }
}
