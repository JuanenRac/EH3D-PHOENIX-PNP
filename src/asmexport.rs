// EH3D-PHOENIX-PNP - the placements in the files that assemblers and other programs ask for.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A board is sent to a service that assembles it, or to another program, and each asks for the positions in its own shape. These write a table of parts in three of them: `cpl`, the
//! component placement list (designator, value, package, mid x, mid y, rotation, layer) that board houses ask for, with the positions in millimetres written with their unit; `kicad-pos`,
//! the text file with columns that KiCad writes and that `parts` reads back; and `generic`, a plain CSV with the unit in the name of each column. The parts are written in the order of their
//! references, with the sides asked for, in millimetres or in inches, and a table that is written can be read again by this program (`kicad-pos` and `generic` keep every number to the
//! digits that are written; the unit of `generic` is in the name of its columns, `x(in)`). The unit and the format never change the board: only how it is written.

use crate::parts::{csv_field, Part, Side};

/// The shapes of file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Cpl,
    KicadPos,
    Generic,
}

impl Format {
    pub const ALL: [Format; 3] = [Format::Cpl, Format::KicadPos, Format::Generic];

    pub fn parse(text: &str) -> Option<Format> {
        match text.trim().to_ascii_lowercase().as_str() {
            "cpl" => Some(Format::Cpl),
            "kicad-pos" | "kicad_pos" => Some(Format::KicadPos),
            "generic" | "csv" => Some(Format::Generic),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Format::Cpl => "cpl",
            Format::KicadPos => "kicad-pos",
            Format::Generic => "generic",
        }
    }
}

/// The unit of the positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Mm,
    Inch,
}

impl Unit {
    pub fn parse(text: &str) -> Option<Unit> {
        match text.trim().to_ascii_lowercase().as_str() {
            "mm" => Some(Unit::Mm),
            "inch" | "in" => Some(Unit::Inch),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Unit::Mm => "mm",
            Unit::Inch => "in",
        }
    }

    fn convert(self, mm: f64) -> f64 {
        match self {
            Unit::Mm => mm,
            Unit::Inch => mm / 25.4,
        }
    }

    fn decimals(self) -> usize {
        match self {
            Unit::Mm => 4,
            Unit::Inch => 6,
        }
    }
}

/// Which sides are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sides {
    All,
    Only(Side),
}

impl Sides {
    pub fn parse(text: &str) -> Option<Sides> {
        match text.trim().to_ascii_lowercase().as_str() {
            "all" => Some(Sides::All),
            "top" => Some(Sides::Only(Side::Top)),
            "bottom" => Some(Sides::Only(Side::Bottom)),
            _ => None,
        }
    }

    fn takes(self, side: Side) -> bool {
        match self {
            Sides::All => true,
            Sides::Only(s) => s == side,
        }
    }
}

fn side_name(side: Side) -> &'static str {
    match side {
        Side::Top => "top",
        Side::Bottom => "bottom",
    }
}

fn capital(side: Side) -> &'static str {
    match side {
        Side::Top => "Top",
        Side::Bottom => "Bottom",
    }
}

/// A turn of any size as a number from 0 up to, not including, 360.
fn turn(deg: f64) -> f64 {
    let r = deg.rem_euclid(360.0);
    if r >= 360.0 - 1e-9 {
        0.0
    } else {
        r
    }
}

/// A text as one word of a file with columns: the spaces become `_` and an empty text becomes `-`.
fn word(text: &str) -> String {
    let t = text.trim().replace(char::is_whitespace, "_");
    if t.is_empty() {
        "-".to_string()
    } else {
        t
    }
}

/// The parts that are written, in the order of their references (the file order breaks a tie).
fn chosen(parts: &[Part], sides: Sides) -> Vec<&Part> {
    let mut list: Vec<&Part> = parts.iter().filter(|p| sides.takes(p.side)).collect();
    list.sort_by(|a, b| a.reference.cmp(&b.reference));
    list
}

/// The table in the shape asked for.
pub fn write(parts: &[Part], format: Format, unit: Unit, sides: Sides) -> String {
    let list = chosen(parts, sides);
    let d = unit.decimals();
    match format {
        Format::Cpl => {
            let mut out = String::from("Designator,Val,Package,Mid X,Mid Y,Rotation,Layer\n");
            for p in list {
                out.push_str(&format!(
                    "{},{},{},{:.d$}{u},{:.d$}{u},{:.2},{}\n",
                    csv_field(&p.reference),
                    csv_field(&p.value),
                    csv_field(&p.package),
                    unit.convert(p.x),
                    unit.convert(p.y),
                    turn(p.rotation),
                    capital(p.side),
                    u = unit.name(),
                ));
            }
            out
        }
        Format::KicadPos => {
            let mut out = format!(
                "### Module positions - created on a board\n## Unit = {}, Angle = deg.\n## Side : {}\n# Ref     Val       Package           PosX       PosY       Rot  Side\n",
                if unit == Unit::Mm { "mm" } else { "inch" },
                match sides {
                    Sides::All => "All",
                    Sides::Only(Side::Top) => "top",
                    Sides::Only(Side::Bottom) => "bottom",
                }
            );
            for p in list {
                out.push_str(&format!(
                    "{:<9} {:<9} {:<17} {:>10.d$} {:>10.d$} {:>8.4}  {}\n",
                    word(&p.reference),
                    word(&p.value),
                    word(&p.package),
                    unit.convert(p.x),
                    unit.convert(p.y),
                    turn(p.rotation),
                    side_name(p.side),
                ));
            }
            out.push_str("## End\n");
            out
        }
        Format::Generic => {
            let mut out = format!(
                "reference,x({u}),y({u}),rotation,side,value,package\n",
                u = unit.name()
            );
            for p in list {
                out.push_str(&format!(
                    "{},{:.d$},{:.d$},{:.4},{},{},{}\n",
                    csv_field(&p.reference),
                    unit.convert(p.x),
                    unit.convert(p.y),
                    turn(p.rotation),
                    side_name(p.side),
                    csv_field(&p.value),
                    csv_field(&p.package),
                ));
            }
            out
        }
    }
}

/// How many parts `write` writes for these sides.
pub fn count(parts: &[Part], sides: Sides) -> usize {
    chosen(parts, sides).len()
}
