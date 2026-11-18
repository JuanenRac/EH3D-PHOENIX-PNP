// EH3D-PHOENIX-PNP - whether the library of feeders agrees with itself.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A table of feeders can be valid row by row and still make no sense as a whole: two feeders whose pick places are on top of each other, a tray with more parts than cells, a strip
//! whose pitch no tape has, a pick place outside the area the machine reaches, the same slot given twice, or a package with no entry in the catalog. This module reads the whole
//! library and lists what does not agree, each finding with the feeder, a severity and the words to fix it. Plain arithmetic: no files, no screen. A feeder that holds its parts in a
//! stick (a tube) or loose (bulk) has one pick place, like a tape: its count is the parts in the stick or in the bin.

use crate::catalog::Catalog;
use crate::feeding::{FeederKind, FeederSpec};
use std::collections::HashMap;

/// The settings of the check, from the configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rules {
    /// The pitch of a strip is a whole number of these (the pocket pitch of a tape is a multiple of 2 mm), in millimetres; 0 does not check the pitch.
    pub pitch_step_mm: f64,
    /// Two pick places closer than this are on top of each other, in millimetres; 0 does not check the gap.
    pub min_gap_mm: f64,
    /// The area the machine reaches.
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
}

impl Default for Rules {
    fn default() -> Self {
        Rules {
            pitch_step_mm: 2.0,
            min_gap_mm: 3.0,
            x_min: -2000.0,
            x_max: 2000.0,
            y_min: -2000.0,
            y_max: 2000.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// The library may work, but it is most likely not what the person meant.
    Warning,
    /// A job with this library cannot do what the table says.
    Error,
}

impl Severity {
    pub fn name(self) -> &'static str {
        match self {
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub severity: Severity,
    pub feeder: String,
    pub message: String,
}

fn add(out: &mut Vec<Finding>, severity: Severity, feeder: &str, message: String) {
    out.push(Finding {
        severity,
        feeder: feeder.to_string(),
        message,
    });
}

/// How many feeders the check looks at: the pairs of pick places are found by sorting, so a long table is not a problem, but a table past this is not a library any more.
pub const MAX_FEEDERS: usize = crate::feeding::MAX_FEEDERS;

/// The places a feeder is picked from now and when its last part is taken: the ends of the line a strip or a tray lays its parts on.
fn ends(f: &FeederSpec) -> Vec<(f64, f64)> {
    let mut v = vec![f.pick_point(0)];
    if !f.kind.is_fixed() {
        v.push(f.pick_point(f.base.count.saturating_sub(1)));
    }
    v
}

/// Checks the whole library. `catalog` is optional: without it the packages are not looked up. The findings come in the order of the table, errors and warnings together.
pub fn check(feeders: &[FeederSpec], catalog: Option<&Catalog>, rules: &Rules) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut slots: HashMap<&str, &str> = HashMap::new();
    for f in feeders {
        let id = f.base.id.as_str();
        if let Some(slot) = f.slot.as_deref() {
            if let Some(first) = slots.insert(slot, id) {
                add(
                    &mut out,
                    Severity::Error,
                    id,
                    format!("the slot '{slot}' is also given to the feeder '{first}'"),
                );
            }
        }
        match f.kind {
            FeederKind::Tape | FeederKind::Tube | FeederKind::Bulk => {}
            FeederKind::Strip { pitch, .. } => {
                if rules.pitch_step_mm > 0.0 {
                    let steps = pitch.abs() / rules.pitch_step_mm;
                    if (steps - steps.round()).abs() * rules.pitch_step_mm > 0.05 || steps < 0.5 {
                        add(
                            &mut out,
                            Severity::Warning,
                            id,
                            format!(
                                "the pitch {pitch} mm is not a whole number of {} mm, the pocket pitch of a tape",
                                rules.pitch_step_mm
                            ),
                        );
                    }
                }
            }
            FeederKind::Tray {
                cols,
                rows,
                pitch_x,
                pitch_y,
                ..
            } => {
                let cells = cols.saturating_mul(rows);
                if f.base.count > cells {
                    add(
                        &mut out,
                        Severity::Error,
                        id,
                        format!(
                            "the tray has {cells} cells and the table says {} parts",
                            f.base.count
                        ),
                    );
                }
                if pitch_x.abs() < 0.5 || pitch_y.abs() < 0.5 {
                    add(
                        &mut out,
                        Severity::Warning,
                        id,
                        format!(
                            "the cells are {} by {} mm apart: no part sits in a cell that small",
                            pitch_x.abs(),
                            pitch_y.abs()
                        ),
                    );
                }
            }
        }
        if f.enabled && f.base.count == 0 {
            add(
                &mut out,
                Severity::Warning,
                id,
                "the feeder is switched on and holds no parts".to_string(),
            );
        }
        if f.warn_below > f.capacity() {
            add(
                &mut out,
                Severity::Warning,
                id,
                format!(
                    "it warns below {} parts and holds at most {}: it warns from the start",
                    f.warn_below,
                    f.capacity()
                ),
            );
        }
        if let Some(cat) = catalog {
            if !cat.is_empty() && cat.package(&f.base.package).is_none() {
                add(
                    &mut out,
                    Severity::Warning,
                    id,
                    format!("the package '{}' is not in the catalog", f.base.package),
                );
            }
        }
        for (x, y) in ends(f) {
            if x < rules.x_min || x > rules.x_max || y < rules.y_min || y > rules.y_max {
                add(
                    &mut out,
                    Severity::Error,
                    id,
                    format!(
                        "a pick place ({x:.2}, {y:.2}) is outside the area the machine reaches"
                    ),
                );
                break;
            }
        }
    }
    if rules.min_gap_mm > 0.0 && feeders.len() <= MAX_FEEDERS {
        clashes(feeders, rules.min_gap_mm, &mut out);
    }
    out
}

/// The feeders whose first pick places are closer than `gap`: the places are sorted along x and each is compared with those that are within `gap` of it along x.
fn clashes(feeders: &[FeederSpec], gap: f64, out: &mut Vec<Finding>) {
    let mut points: Vec<(f64, f64, usize)> = feeders
        .iter()
        .enumerate()
        .filter(|(_, f)| f.enabled)
        .map(|(i, f)| {
            let p = f.pick_point(0);
            (p.0, p.1, i)
        })
        .collect();
    points.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    for (n, a) in points.iter().enumerate() {
        for b in points.iter().skip(n + 1) {
            if b.0 - a.0 >= gap {
                break;
            }
            let d = (b.0 - a.0).hypot(b.1 - a.1);
            if d < gap {
                let (first, second) = (a.2.min(b.2), a.2.max(b.2));
                add(
                    out,
                    Severity::Error,
                    &feeders[second].base.id,
                    format!(
                        "its pick place is {d:.2} mm from the one of '{}': less than the {gap} mm between feeders",
                        feeders[first].base.id
                    ),
                );
            }
        }
    }
}

/// How many errors and warnings.
pub fn counts(findings: &[Finding]) -> (usize, usize) {
    let errors = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();
    (errors, findings.len() - errors)
}

/// The findings as text, one line each, errors first.
pub fn text(findings: &[Finding]) -> String {
    let mut sorted: Vec<&Finding> = findings.iter().collect();
    sorted.sort_by_key(|f| std::cmp::Reverse(f.severity));
    let mut out = String::new();
    for f in sorted {
        out.push_str(&format!(
            "{}: feeder {}: {}\n",
            f.severity.name(),
            f.feeder,
            f.message
        ));
    }
    let (e, w) = counts(findings);
    out.push_str(&format!("{e} errors, {w} warnings\n"));
    out
}
