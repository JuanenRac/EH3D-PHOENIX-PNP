// EH3D-PHOENIX-PNP - the position files of the CAD programs brought to one origin and one way of counting the rotation.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The programs that draw a board do not agree on where the origin is, on which way the Y axis goes nor on which way the rotation is counted: a file may count from the corner of the
//! board, from a point that the person put on it (the auxiliary origin) or from the corner of the sheet; EAGLE writes a part of the bottom side as `MR90`; a house may want the rotation
//! of the bottom side mirrored or turned half a turn. `detect` names the dialect of a file (KiCad, Altium, EAGLE, the list of an assembly house or a plain table), `read` gives its parts in
//! millimetres and degrees and `convert` moves the origin, flips the Y axis, changes the way the rotation is counted, adds a turn and applies the rule of the bottom side. The order is
//! always the same: the place first (the origin moves, then Y flips), then the rotation (the direction, then the turn, then the bottom side), and `undo` gives back what `convert` made.

use crate::asmimport;
use crate::error::{Code, PnpError};
use crate::import::{self, Format as ImportFormat};
use crate::jlcpcb;
use crate::parts::{self, BottomRotation, Part, Profile, Side};
use std::collections::HashSet;

/// The largest place that a conversion may give, in millimetres.
const MAX_MM: f64 = 1_000_000.0;

/// The dialects of a position file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Kicad,
    Altium,
    Eagle,
    /// The list of an assembly house: `Designator`, `Mid X`, `Mid Y`, `Layer`, `Rotation`.
    Jlcpcb,
    /// Any other table with a header.
    Generic,
}

impl Dialect {
    pub const ALL: [Dialect; 5] = [
        Dialect::Kicad,
        Dialect::Altium,
        Dialect::Eagle,
        Dialect::Jlcpcb,
        Dialect::Generic,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Dialect::Kicad => "kicad",
            Dialect::Altium => "altium",
            Dialect::Eagle => "eagle",
            Dialect::Jlcpcb => "jlcpcb",
            Dialect::Generic => "generic",
        }
    }

    pub fn parse(text: &str) -> Option<Dialect> {
        let t = text.trim().to_ascii_lowercase();
        Dialect::ALL.into_iter().find(|d| d.name() == t)
    }
}

/// The dialect that the header (or the first row) of a text shows.
pub fn detect(text: &str) -> Dialect {
    let first = text
        .trim_start_matches('\u{feff}')
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    let low = first.to_ascii_lowercase();
    if low.trim_start().starts_with('#') {
        return Dialect::Kicad;
    }
    if !first.contains([',', ';', '\t']) {
        return match import::detect(text) {
            Ok(ImportFormat::Eagle) => Dialect::Eagle,
            Ok(ImportFormat::KicadPos) => Dialect::Kicad,
            _ => Dialect::Generic,
        };
    }
    let squash: String = low
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '_'))
        .collect();
    if squash.contains("centerx") {
        Dialect::Altium
    } else if squash.contains("midx") {
        Dialect::Jlcpcb
    } else if squash.contains("posx") {
        Dialect::Kicad
    } else {
        Dialect::Generic
    }
}

/// What a file holds, in millimetres and degrees.
#[derive(Debug, Clone, PartialEq)]
pub struct Read {
    pub dialect: Dialect,
    pub parts: Vec<Part>,
    /// The parts that the file marks as not to be mounted.
    pub dnp: Vec<Part>,
}

/// Reads a position file. The dialect is told from the text when `dialect` is `None`; a file that names its units in the header (`Center-X(mil)`) or in its numbers (`12.5mm`) is converted.
/// The column that marks the parts that are not to be mounted is the one that `export-fab` writes by default (`Customer Supply`, with `DNP` in the line of such a part).
pub fn read(text: &str, dialect: Option<Dialect>) -> Result<Read, PnpError> {
    read_with(text, dialect, "Customer Supply")
}

/// The same, with the name of the column that marks the parts that are not to be mounted (the setting `fab_dnp_column`).
pub fn read_with(text: &str, dialect: Option<Dialect>, dnp_column: &str) -> Result<Read, PnpError> {
    let found = dialect.unwrap_or_else(|| detect(text));
    let as_text = matches!(found, Dialect::Eagle)
        || (found == Dialect::Kicad
            && text
                .trim_start_matches('\u{feff}')
                .trim_start()
                .starts_with('#'));
    if as_text {
        let imported =
            import::import(text, ImportFormat::Auto, None, &import::Settings::default())?;
        return Ok(Read {
            dialect: found,
            parts: imported.parts(),
            dnp: imported.dnp_parts(),
        });
    }
    let profile = Profile {
        extra_dnp: vec![dnp_column.to_string()],
        extra_yes: vec![jlcpcb::DNP_MARK.to_ascii_lowercase()],
        ..Profile::default()
    };
    match parts::parse_all(text, &profile) {
        Ok(p) => Ok(Read {
            dialect: found,
            parts: p.parts,
            dnp: p.dnp,
        }),
        Err(first) => match asmimport::parse(text) {
            Ok(i) => {
                let off = marked_references(text, dnp_column);
                let (dnp, parts): (Vec<Part>, Vec<Part>) = i
                    .parts
                    .into_iter()
                    .partition(|p| off.contains(&p.reference));
                Ok(Read {
                    dialect: found,
                    parts,
                    dnp,
                })
            }
            Err(e) => Err(PnpError::new(
                Code::ImportBad,
                format!("{first}; read as a list with units: {e}"),
            )),
        },
    }
}

/// The designators whose line has a yes (`DNP`, `yes`, `y`, `true`, `1`, `x`) in the column called `column`, for a list that is read with its units in the numbers.
fn marked_references(text: &str, column: &str) -> HashSet<String> {
    let text = text.trim_start_matches('\u{feff}');
    let head = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let Ok(records) = parts::read_records_numbered(text, parts::detect_delimiter(head)) else {
        return HashSet::new();
    };
    let mut rows = records.into_iter();
    let Some((_, header)) = rows.next() else {
        return HashSet::new();
    };
    let at = |names: &[&str]| {
        header.iter().position(|h| {
            let h = h.trim().to_ascii_lowercase();
            names.contains(&h.as_str())
        })
    };
    let wanted = column.trim().to_ascii_lowercase();
    let (Some(reference), Some(mark)) = (
        at(&["designator", "reference", "ref", "refdes"]),
        at(&[wanted.as_str()]),
    ) else {
        return HashSet::new();
    };
    rows.filter(|(_, row)| {
        row.get(mark).is_some_and(|c| {
            matches!(
                c.trim().to_ascii_lowercase().as_str(),
                "dnp" | "yes" | "y" | "true" | "1" | "x"
            )
        })
    })
    .filter_map(|(_, row)| row.get(reference).map(|r| r.trim().to_string()))
    .collect()
}

/// What a conversion does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Conversion {
    /// The point of the file (mm) that becomes the origin.
    pub origin_x: f64,
    pub origin_y: f64,
    /// Y goes the other way.
    pub flip_y: bool,
    /// The file counts the rotation clockwise.
    pub clockwise: bool,
    /// A turn added to every part (degrees, after the direction is set right).
    pub rotation_offset: f64,
    /// What is done with the rotation of the parts of the bottom side, last.
    pub bottom: BottomRotation,
}

impl Default for Conversion {
    fn default() -> Conversion {
        Conversion {
            origin_x: 0.0,
            origin_y: 0.0,
            flip_y: false,
            clockwise: false,
            rotation_offset: 0.0,
            bottom: BottomRotation::Keep,
        }
    }
}

impl Conversion {
    /// Whether the conversion changes nothing.
    pub fn is_identity(&self) -> bool {
        *self == Conversion::default()
    }

    fn check(&self) -> Result<(), PnpError> {
        let finite = [self.origin_x, self.origin_y, self.rotation_offset]
            .iter()
            .all(|v| v.is_finite());
        if !finite || self.origin_x.abs() > MAX_MM || self.origin_y.abs() > MAX_MM {
            return Err(PnpError::new(
                Code::PartsBad,
                format!(
                    "the origin ({}, {}) and the turn {} are numbers, the origin within {MAX_MM} mm",
                    self.origin_x, self.origin_y, self.rotation_offset
                ),
            ));
        }
        Ok(())
    }

    /// Whether the rotation changes sign: when the file counts clockwise, or when Y flips (a mirror changes the way angles go), but not when both happen.
    fn negates(&self) -> bool {
        self.clockwise != self.flip_y
    }
}

fn norm(deg: f64) -> f64 {
    let r = deg.rem_euclid(360.0);
    if r >= 360.0 - 1e-9 {
        0.0
    } else {
        r
    }
}

fn bottom_of(rule: BottomRotation, part: &Part, rotation: f64) -> f64 {
    if part.side == Side::Bottom {
        rule.apply(rotation)
    } else {
        rotation
    }
}

/// The parts as the conversion makes them. A conversion that would put a part farther than a million millimetres from the origin, or that has a number that is not finite, is an error.
pub fn convert(parts: &[Part], c: &Conversion) -> Result<Vec<Part>, PnpError> {
    c.check()?;
    let mut out = Vec::with_capacity(parts.len());
    for p in parts {
        let x = p.x - c.origin_x;
        let y0 = p.y - c.origin_y;
        let y = if c.flip_y { -y0 } else { y0 };
        if x.abs() > MAX_MM || y.abs() > MAX_MM {
            return Err(PnpError::new(
                Code::PartsBad,
                format!(
                    "{} would be at ({x}, {y}): farther than {MAX_MM} mm",
                    p.reference
                ),
            ));
        }
        let signed = if c.negates() { -p.rotation } else { p.rotation };
        let rotation = norm(bottom_of(c.bottom, p, norm(signed + c.rotation_offset)));
        out.push(Part {
            x,
            y,
            rotation,
            ..p.clone()
        });
    }
    Ok(out)
}

/// What `convert` did, undone: `undo(convert(p, c), c)` gives `p` back (to the digits of a float).
pub fn undo(parts: &[Part], c: &Conversion) -> Result<Vec<Part>, PnpError> {
    c.check()?;
    Ok(parts
        .iter()
        .map(|p| {
            let before = norm(bottom_of(c.bottom, p, p.rotation) - c.rotation_offset);
            let rotation = norm(if c.negates() { -before } else { before });
            Part {
                x: p.x + c.origin_x,
                y: if c.flip_y { -p.y } else { p.y } + c.origin_y,
                rotation,
                ..p.clone()
            }
        })
        .collect())
}
