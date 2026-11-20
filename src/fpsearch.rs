// EH3D-PHOENIX-PNP - the names of the footprints brought to one form, and a search in a library of them.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The same footprint is written in many ways: `R_0603`, `Resistor_SMD:R_0603_1608Metric`, `RESC1608X55N`, `0603`, `r0603`. `key` brings a name to a form in which they are alike: the
//! library in front of a colon and the metric suffix of KiCad go, a name of the convention of IPC-7351B and an EIA chip code are both the body they name (`chip 1.6x0.8`), and any other
//! name is its letters and digits in capitals. `search` looks a name up in a list of names: first the same text, then the same footprint under another name, then a name that holds the
//! words, then names a few letters away (the distance is a setting). It reads names and changes nothing.

use crate::ipcname::{self, Shape};
use crate::pkggen;
use std::collections::HashSet;

/// The most names a library may have for a search, and the most characters of a name that are looked at.
pub const MAX_LIBRARY: usize = 1_000_000;
const MAX_CHARS: usize = 128;

/// How a name matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum How {
    /// The same text, without regard to case.
    Exact,
    /// Another name for the same footprint.
    SameFootprint,
    /// The name holds the words that were asked for.
    Contains,
    /// A few letters away.
    Near,
}

impl How {
    pub fn name(self) -> &'static str {
        match self {
            How::Exact => "exact",
            How::SameFootprint => "same footprint",
            How::Contains => "contains",
            How::Near => "near",
        }
    }
}

/// A name of the library that matched.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub name: String,
    pub how: How,
    /// The number of letters that differ (0 unless `how` is `Near`).
    pub distance: usize,
}

/// A name without the library before a colon and without the metric size that KiCad adds (`_1608Metric`).
fn bare(name: &str) -> String {
    let t = name.trim();
    let t = t.rsplit(':').next().unwrap_or(t);
    let t: String = t.chars().take(MAX_CHARS).collect();
    let kept: Vec<&str> = t
        .split('_')
        .filter(|p| !p.to_ascii_lowercase().ends_with("metric"))
        .collect();
    kept.join("_")
}

/// The form in which names of one footprint are alike. A chip is `chip:LxW:FAMILY` (the body with one decimal, longest side first, and the family of its kind, which is empty when the name
/// does not say it: `0603`); a name of the convention keeps its family and sizes; any other name is its letters and digits in capitals.
pub fn key(name: &str) -> String {
    let stripped = bare(name);
    if let Ok(ipc) = ipcname::parse(&stripped) {
        return match ipc.shape {
            Shape::Chip {
                length_mm,
                width_mm,
                ..
            } => format!(
                "chip:{:.1}x{:.1}:{}",
                length_mm.max(width_mm),
                length_mm.min(width_mm),
                ipc.family
            ),
            Shape::Dual {
                pitch_mm,
                span_mm,
                pins,
                ..
            } => format!("{} {pitch_mm:.2}p{span_mm:.2} {pins}", ipc.family),
            Shape::Quad {
                pitch_mm,
                length_mm,
                width_mm,
                pins,
                ..
            } => format!(
                "{} {pitch_mm:.2}p{length_mm:.2}x{width_mm:.2} {pins}",
                ipc.family
            ),
        };
    }
    let digits_end = stripped
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .count();
    if (4..=5).contains(&digits_end) {
        if let Ok(spec) = pkggen::generate(&stripped) {
            return format!(
                "chip:{:.1}x{:.1}:{}",
                spec.body_long_mm,
                spec.body_short_mm,
                ipcname::chip_family(&stripped).unwrap_or("")
            );
        }
    }
    stripped
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_uppercase()
}

/// Whether two keys are one footprint: the same key, or two chips of one body of which one does not say its kind.
pub fn same_footprint(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let chip = |k: &str| -> Option<(String, String)> {
        let rest = k.strip_prefix("chip:")?;
        let (size, family) = rest.split_once(':')?;
        Some((size.to_string(), family.to_string()))
    };
    match (chip(a), chip(b)) {
        (Some((sa, fa)), Some((sb, fb))) => sa == sb && (fa.is_empty() || fb.is_empty()),
        _ => false,
    }
}

/// The number of single-letter changes (insert, delete, replace) that turn one text into the other, or `None` when it is more than `max`.
pub fn distance(a: &str, b: &str, max: usize) -> Option<usize> {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.len().abs_diff(b.len()) > max {
        return None;
    }
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        let mut best = row[0];
        for (j, cb) in b.iter().enumerate() {
            let up = row[j + 1];
            row[j + 1] = (diagonal + usize::from(ca != cb))
                .min(row[j] + 1)
                .min(up + 1);
            diagonal = up;
            best = best.min(row[j + 1]);
        }
        if best > max {
            return None;
        }
    }
    Some(row[b.len()]).filter(|d| *d <= max)
}

/// The compact form used to compare words: letters and digits in capitals.
fn compact(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_uppercase()
}

/// Looks `query` up in `library`. The hits are in the order exact, same footprint, contains, near (the nearest first), the names in the order of the library inside each, and a name
/// is only once in the list; at most `limit` are given. `max_distance` is how many letters a name may differ by to be called near (0 turns that off).
pub fn search(library: &[String], query: &str, max_distance: usize, limit: usize) -> Vec<Hit> {
    let want = query.trim().to_ascii_lowercase();
    if want.is_empty() {
        return Vec::new();
    }
    let want_key = key(query);
    let want_compact = compact(query);
    let mut hits: Vec<(Hit, usize)> = Vec::new();
    let mut seen = HashSet::new();
    for (index, name) in library.iter().take(MAX_LIBRARY).enumerate() {
        if !seen.insert(name.as_str()) {
            continue;
        }
        let hit = if name.trim().to_ascii_lowercase() == want {
            Some((How::Exact, 0))
        } else if same_footprint(&key(name), &want_key) {
            Some((How::SameFootprint, 0))
        } else if !want_compact.is_empty() && compact(name).contains(&want_compact) {
            Some((How::Contains, 0))
        } else {
            distance(&compact(name), &want_compact, max_distance)
                .filter(|_| max_distance > 0)
                .map(|d| (How::Near, d))
        };
        if let Some((how, d)) = hit {
            hits.push((
                Hit {
                    name: name.clone(),
                    how,
                    distance: d,
                },
                index,
            ));
        }
    }
    hits.sort_by_key(|a| (a.0.how, a.0.distance, a.1));
    hits.into_iter().take(limit).map(|(h, _)| h).collect()
}

/// The hits as lines of text: the name, how it matched and, for a near name, how many letters differ.
pub fn text(hits: &[Hit]) -> String {
    let mut out = String::new();
    for h in hits {
        if h.how == How::Near {
            out.push_str(&format!(
                "{}  ({}, {} letters)\n",
                h.name,
                h.how.name(),
                h.distance
            ));
        } else {
            out.push_str(&format!("{}  ({})\n", h.name, h.how.name()));
        }
    }
    out
}
