// EH3D-PHOENIX-PNP - marks of two levels and boards marked as bad.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The marks that a panel is aligned with, in two levels, and the parts that are marks.
//!
//! The first level is the panel: the marks of the panel file, fitted as a whole. The second level is the board: a part of a board that is a mark (a fiducial part: its value or its
//! package holds a word of `fiducial_words`, or its reference is a prefix of `fiducial_refs` and a number, such as `FID1`) is not placed, it is a mark of its board, named
//! `board/reference` (`A/FID1`, or `B/C/FID2` for a board inside a panel inside the panel). A measured mark names the mark it measures, and the marks of one board are fitted
//! together: the parts of that board are moved by their own fit, the parts of the other boards by the fit of the panel. Everything here is plain arithmetic and text, with no files and
//! no screen.

use crate::error::{Code, PnpError};
use crate::fiducial::{self, Fit, FitMode};
use crate::parts::Part;
use std::collections::HashSet;

/// The marks that fitted one level: the panel (an empty group) or one board (its path in the panel, such as `A` or `B/C`).
#[derive(Debug, Clone, PartialEq)]
pub struct LevelFit {
    /// The board the fit belongs to; empty for the panel.
    pub group: String,
    pub fit: Fit,
}

fn bad(message: impl Into<String>) -> PnpError {
    PnpError::new(Code::FiducialBad, message)
}

/// How many boards deep a group is: the panel is 0, `A` is 1, `B/C` is 2.
fn depth(group: &str) -> usize {
    if group.is_empty() {
        0
    } else {
        group.matches('/').count() + 1
    }
}

/// The board a mark of a board belongs to: everything before its last `/`.
pub fn group_of(id: &str) -> &str {
    id.rsplit_once('/').map_or("", |(group, _)| group)
}

/// Whether a part is a mark and not a part to mount. `words` and `refs` are in lower case. The last word of the reference (after the last `/`) is the one that is looked at, so a
/// part of a board inside a panel is judged by its own reference.
pub fn is_fiducial_part(part: &Part, words: &[String], refs: &[String]) -> bool {
    let reference = part.reference.rsplit('/').next().unwrap_or("");
    let reference = reference.to_ascii_lowercase();
    let by_reference = refs.iter().any(|prefix| {
        !prefix.is_empty()
            && reference
                .strip_prefix(prefix.as_str())
                .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
    });
    if by_reference {
        return true;
    }
    let value = part.value.to_ascii_lowercase();
    let package = part.package.to_ascii_lowercase();
    words
        .iter()
        .any(|w| !w.is_empty() && (value.contains(w.as_str()) || package.contains(w.as_str())))
}

/// Takes the parts that are marks out of the list and gives them back, in the order they had.
pub fn take_fiducial_parts(parts: &mut Vec<Part>, words: &[String], refs: &[String]) -> Vec<Part> {
    let mut taken = Vec::new();
    let mut kept = Vec::with_capacity(parts.len());
    for part in parts.drain(..) {
        if is_fiducial_part(&part, words, refs) {
            taken.push(part);
        } else {
            kept.push(part);
        }
    }
    *parts = kept;
    taken
}

/// The marks that parts are, as places with names: `(reference, x, y)`.
pub fn marks_of(parts: &[Part]) -> Vec<(String, f64, f64)> {
    parts
        .iter()
        .map(|p| (p.reference.clone(), p.x, p.y))
        .collect()
}

/// Fits each level that has measured marks. `panel` are the marks of the panel file and `boards` the marks that are parts (named `board/reference`), both at the places of the
/// design; `measured` are the places measured on the machine, each named after the mark it measures. A measured mark that neither level has is an error (most likely a typing
/// mistake). The levels come in order of depth: the panel first, then the boards. A board with measured marks needs at least two of them, and a mark further than
/// `max_residual_mm` from where its fit puts it refuses that fit.
pub fn fit_levels(
    panel: &[(String, f64, f64)],
    boards: &[(String, f64, f64)],
    measured: &[(String, f64, f64)],
    mode: FitMode,
    max_residual_mm: f64,
) -> Result<Vec<LevelFit>, PnpError> {
    let in_panel: HashSet<&str> = panel.iter().map(|m| m.0.as_str()).collect();
    let in_boards: HashSet<&str> = boards.iter().map(|m| m.0.as_str()).collect();
    let mut groups: Vec<&str> = Vec::new();
    for m in measured {
        let id = m.0.as_str();
        let group = if in_panel.contains(id) {
            ""
        } else if in_boards.contains(id) {
            group_of(id)
        } else {
            return Err(bad(format!(
                "the mark '{id}' is not in the panel or in a board"
            )));
        };
        if !groups.contains(&group) {
            groups.push(group);
        }
    }
    groups.sort_by(|a, b| depth(a).cmp(&depth(b)).then(a.cmp(b)));
    let mut out = Vec::new();
    for group in groups {
        let design: Vec<(String, f64, f64)> = if group.is_empty() {
            panel.to_vec()
        } else {
            boards
                .iter()
                .filter(|m| group_of(&m.0) == group)
                .cloned()
                .collect()
        };
        let seen: Vec<(String, f64, f64)> = measured
            .iter()
            .filter(|m| {
                if group.is_empty() {
                    in_panel.contains(m.0.as_str())
                } else {
                    !in_panel.contains(m.0.as_str()) && group_of(&m.0) == group
                }
            })
            .cloned()
            .collect();
        let fit = fiducial::solve(&design, &seen, mode, max_residual_mm).map_err(|e| {
            if group.is_empty() {
                e
            } else {
                bad(format!("board {group}: {}", e.detail))
            }
        })?;
        out.push(LevelFit {
            group: group.to_string(),
            fit,
        });
    }
    Ok(out)
}

/// Moves the parts: each one by the fit of the deepest level it belongs to (the fit of its board when its board has one, else the fit of the panel). A part is a part of the board
/// `A` when its reference starts with `A/`.
pub fn correct_levels(parts: &mut [Part], fits: &[LevelFit]) {
    for part in parts.iter_mut() {
        let best = fits
            .iter()
            .filter(|f| {
                f.group.is_empty()
                    || part
                        .reference
                        .strip_prefix(f.group.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
            })
            .max_by_key(|f| depth(&f.group));
        if let Some(level) = best {
            fiducial::correct(std::slice::from_mut(part), &level.fit.transform);
        }
    }
}

/// Reads a list of boards written on the command line, `A,B/C`: names separated by commas, spaces around them dropped, empty names left out.
pub fn parse_list(text: &str) -> Vec<String> {
    text.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}
