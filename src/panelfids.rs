// EH3D-PHOENIX-PNP - the marks of each board of a panel, in the coordinates of the panel.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A panel is aligned twice: by its own marks and, for the best placement, by the marks of each board, because boards in a panel are never exactly where the panel says. The marks of a board are
//! parts of its file (`FID1`, `FID2`, or a part whose value or package holds a word of `fiducial_words`), and once the board is put in its place in the panel they have the places the camera has to
//! look at. This lists them for every board of the panel, in the coordinates of the panel, with the turn and the place of the board counted in, and says which boards cannot be aligned: a board
//! with no mark, a board with only one (a turn cannot be told from one point), and a board whose marks are closer than a distance (the turn is then known only as well as the marks are apart).
//! A board marked as bad (X-out) is left out, with its marks. Plain arithmetic: nothing is read from a machine.

use crate::config::MachineConfig;
use crate::error::PnpError;
use crate::marks::{group_of, is_fiducial_part};
use crate::panel::Panel;
use crate::parts::{csv_field, Part};
use std::collections::BTreeMap;

/// What a board comes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Two marks or more, the farthest two at the least distance apart.
    Ok,
    /// The board has no mark.
    NoMark,
    /// The board has one mark: it cannot tell a turn.
    OneMark,
    /// The farthest two marks are closer than the least distance.
    TooClose,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::NoMark => "no_mark",
            Status::OneMark => "one_mark",
            Status::TooClose => "too_close",
        }
    }
}

/// One board of the panel (by its path, `A` or `B/C`).
#[derive(Debug, Clone, PartialEq)]
pub struct BoardMarks {
    pub board: String,
    /// The marks as `(reference, x, y)` in the coordinates of the panel, in the order of the file.
    pub marks: Vec<(String, f64, f64)>,
    /// The distance between the two marks that are farthest apart (0 with fewer than two).
    pub widest_mm: f64,
    pub status: Status,
}

/// The boards of a panel.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub boards: Vec<BoardMarks>,
    /// The boards that are marked as bad and left out.
    pub left_out: Vec<String>,
    pub min_sep_mm: f64,
}

/// The marks of every board among the parts of a panel that was expanded (the parts are named `board/reference`). `words` and `refs` are the ones of the settings, in lower case. The boards
/// are the ones that have a part, mark or not, in the order of their paths.
pub fn analyse(
    parts: &[Part],
    left_out: &[String],
    words: &[String],
    refs: &[String],
    min_sep_mm: f64,
) -> Report {
    let mut by: BTreeMap<String, Vec<(String, f64, f64)>> = BTreeMap::new();
    for p in parts {
        let board = group_of(&p.reference).to_string();
        let marks = by.entry(board).or_default();
        if is_fiducial_part(p, words, refs) {
            marks.push((p.reference.clone(), p.x, p.y));
        }
    }
    let boards = by
        .into_iter()
        .map(|(board, marks)| {
            let mut widest = 0.0f64;
            for (i, a) in marks.iter().enumerate() {
                for b in &marks[i + 1..] {
                    widest = widest.max((a.1 - b.1).hypot(a.2 - b.2));
                }
            }
            let status = match marks.len() {
                0 => Status::NoMark,
                1 => Status::OneMark,
                _ if widest < min_sep_mm => Status::TooClose,
                _ => Status::Ok,
            };
            BoardMarks {
                board,
                marks,
                widest_mm: widest,
                status,
            }
        })
        .collect();
    Report {
        boards,
        left_out: left_out.to_vec(),
        min_sep_mm,
    }
}

/// Puts the boards of a panel in their places (`load` gives the parts of a board by its file, `x_out` leaves more boards out by their path) and lists their marks.
pub fn from_panel(
    panel: &Panel,
    load: &dyn Fn(&str) -> Result<Vec<Part>, String>,
    x_out: &[String],
    cfg: &MachineConfig,
) -> Result<Report, PnpError> {
    let expanded = panel.expand_with(load, x_out)?;
    Ok(analyse(
        &expanded.parts,
        &expanded.x_out,
        &cfg.fiducial_words,
        &cfg.fiducial_refs,
        cfg.panel_fid_min_sep_mm,
    ))
}

impl Report {
    /// The boards that cannot be aligned by their own marks.
    pub fn problems(&self) -> usize {
        self.boards
            .iter()
            .filter(|b| b.status != Status::Ok)
            .count()
    }

    /// The report for a person: a line for each board and a line that sums it up.
    pub fn text(&self) -> String {
        let mut out = format!(
            "{} boards in the panel, the marks of a board must be {} mm apart at the least\n",
            self.boards.len(),
            self.min_sep_mm
        );
        for b in &self.boards {
            let marks: Vec<String> = b
                .marks
                .iter()
                .map(|m| format!("{} ({:.3}, {:.3})", m.0, m.1, m.2))
                .collect();
            out.push_str(&format!(
                "{}: {} marks, the farthest {:.3} mm apart - {}{}{}\n",
                if b.board.is_empty() {
                    "(panel)"
                } else {
                    &b.board
                },
                b.marks.len(),
                b.widest_mm,
                b.status.name(),
                if marks.is_empty() { "" } else { ": " },
                marks.join(", ")
            ));
        }
        for path in &self.left_out {
            out.push_str(&format!("{path}: marked as bad and left out\n"));
        }
        match self.problems() {
            0 => out.push_str("every board can be aligned by its own marks\n"),
            n => out.push_str(&format!(
                "{n} boards cannot be aligned by their own marks\n"
            )),
        }
        out
    }

    /// The table `board,mark,x,y,status`, a row for each mark (a board with none has a row with an empty mark).
    pub fn csv(&self) -> String {
        let mut out = String::from("board,mark,x,y,status\n");
        for b in &self.boards {
            if b.marks.is_empty() {
                out.push_str(&format!("{},,,,{}\n", csv_field(&b.board), b.status.name()));
            }
            for m in &b.marks {
                out.push_str(&format!(
                    "{},{},{:.4},{:.4},{}\n",
                    csv_field(&b.board),
                    csv_field(&m.0),
                    m.1,
                    m.2,
                    b.status.name()
                ));
            }
        }
        out
    }
}
