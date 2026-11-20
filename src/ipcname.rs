// EH3D-PHOENIX-PNP - the names of the footprints by the public convention of IPC-7351B.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The convention writes the shape of a footprint in its name: the letters of the family (`RESC` a chip resistor, `CAPC`, `INDC`, `LEDC`, `SOIC`, `SOT`, `QFP`, `QFN`, ...), the sizes in
//! hundredths of a millimetre, the pins and a letter for the density of the land: `L` least, `N` nominal, `M` most. A chip is `RESC1608X55N` (the body 1.6 by 0.8 mm written in tenths as
//! `16` and `08`, the height 0.55 mm as `55`); a part with a row of leads on two sides is `SOIC127P600X175-8N` (the pitch 1.27, the span over the leads 6.00, the height 1.75, eight pins);
//! a part with leads on four sides is `QFP50P1000X1000X160-48N` (the pitch 0.5, the body 10.00 by 10.00, the height 1.60, 48 pins). This writes and reads those names and nothing else:
//! it holds no table of the land patterns, and the sizes of the lands are not computed. A name made here is not checked against the standard.

use crate::error::{Code, PnpError};
use crate::pkggen;

/// The density of the land.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Density {
    Least,
    #[default]
    Nominal,
    Most,
}

impl Density {
    pub const ALL: [Density; 3] = [Density::Least, Density::Nominal, Density::Most];

    /// `L`, `N` or `M`.
    pub fn letter(self) -> char {
        match self {
            Density::Least => 'L',
            Density::Nominal => 'N',
            Density::Most => 'M',
        }
    }

    /// The density a letter or a word names: `L`, `N`, `M`, `least`, `nominal`, `most`, and the levels of the older standard `C` (least), `B` (nominal) and `A` (most).
    pub fn parse(text: &str) -> Option<Density> {
        match text.trim().to_ascii_lowercase().as_str() {
            "l" | "least" | "c" => Some(Density::Least),
            "n" | "nominal" | "b" => Some(Density::Nominal),
            "m" | "most" | "a" => Some(Density::Most),
            _ => None,
        }
    }
}

/// The sizes of a footprint, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    /// A body with a terminal at each end.
    Chip {
        length_mm: f64,
        width_mm: f64,
        height_mm: f64,
    },
    /// Leads on two sides.
    Dual {
        pitch_mm: f64,
        span_mm: f64,
        height_mm: f64,
        pins: u32,
    },
    /// Leads on four sides.
    Quad {
        pitch_mm: f64,
        length_mm: f64,
        width_mm: f64,
        height_mm: f64,
        pins: u32,
    },
}

/// A name of the convention taken apart.
#[derive(Debug, Clone, PartialEq)]
pub struct IpcName {
    /// The letters of the family, in capitals.
    pub family: String,
    pub shape: Shape,
    pub density: Density,
}

fn bad(message: impl Into<String>) -> PnpError {
    PnpError::new(Code::CatalogBadRow, message)
}

/// The most that a size may be, in millimetres, and the most pins.
const MAX_MM: f64 = 999.0;
const MAX_PINS: u32 = 10_000;

/// A size in hundredths of a millimetre, as a whole number; an error when it is not a number from 0.01 up to the maximum.
fn hundredths(mm: f64, what: &str) -> Result<u32, PnpError> {
    if !mm.is_finite() || !(0.01..=MAX_MM).contains(&mm) {
        return Err(bad(format!(
            "the {what} {mm} is a number of millimetres from 0.01 to {MAX_MM}"
        )));
    }
    Ok((mm * 100.0).round() as u32)
}

/// A size in tenths of a millimetre for a chip: 0.1 up to 9.9, written with two digits.
fn tenths(mm: f64, what: &str) -> Result<String, PnpError> {
    if !mm.is_finite() || !(0.05..9.95).contains(&mm) {
        return Err(bad(format!(
            "the {what} {mm} of a chip is a number of millimetres from 0.1 to 9.9"
        )));
    }
    Ok(format!("{:02}", (mm * 10.0).round() as u32))
}

fn family_ok(family: &str) -> Result<(), PnpError> {
    if (2..=8).contains(&family.len()) && family.chars().all(|c| c.is_ascii_uppercase()) {
        Ok(())
    } else {
        Err(bad(format!(
            "the family '{family}' is two to eight capital letters (RESC, CAPC, SOIC, QFP, ...)"
        )))
    }
}

impl IpcName {
    /// The name written. An error for a size that cannot be written (a chip over 9.9 mm, a number of pins of none or over 10000) or a family that is not capital letters.
    pub fn name(&self) -> Result<String, PnpError> {
        family_ok(&self.family)?;
        let pins = |n: u32| {
            if (1..=MAX_PINS).contains(&n) {
                Ok(n)
            } else {
                Err(bad(format!("{n} pins: from 1 to {MAX_PINS}")))
            }
        };
        let d = self.density.letter();
        Ok(match self.shape {
            Shape::Chip {
                length_mm,
                width_mm,
                height_mm,
            } => format!(
                "{}{}{}X{}{d}",
                self.family,
                tenths(length_mm, "length")?,
                tenths(width_mm, "width")?,
                hundredths(height_mm, "height")?
            ),
            Shape::Dual {
                pitch_mm,
                span_mm,
                height_mm,
                pins: n,
            } => format!(
                "{}{}P{}X{}-{}{d}",
                self.family,
                hundredths(pitch_mm, "pitch")?,
                hundredths(span_mm, "span")?,
                hundredths(height_mm, "height")?,
                pins(n)?
            ),
            Shape::Quad {
                pitch_mm,
                length_mm,
                width_mm,
                height_mm,
                pins: n,
            } => format!(
                "{}{}P{}X{}X{}-{}{d}",
                self.family,
                hundredths(pitch_mm, "pitch")?,
                hundredths(length_mm, "length")?,
                hundredths(width_mm, "width")?,
                hundredths(height_mm, "height")?,
                pins(n)?
            ),
        })
    }
}

fn whole(text: &str, name: &str) -> Result<u32, PnpError> {
    if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) || text.len() > 7 {
        return Err(bad(format!(
            "'{name}': '{text}' is not a size written in digits"
        )));
    }
    text.parse()
        .map_err(|_| bad(format!("'{name}': '{text}' is too big")))
}

/// Takes a name apart. The density letter is the last one and is needed; the family is the capital letters at the start.
pub fn parse(text: &str) -> Result<IpcName, PnpError> {
    let name = text.trim().to_ascii_uppercase();
    let Some(last) = name.chars().next_back() else {
        return Err(bad("the name is empty"));
    };
    let density = match last {
        'L' | 'N' | 'M' => Density::parse(&last.to_string()).unwrap_or_default(),
        _ => {
            return Err(bad(format!(
                "'{name}' does not end with the letter of the density (L, N or M)"
            )))
        }
    };
    let body = &name[..name.len() - 1];
    let at = body
        .find(|c: char| !c.is_ascii_uppercase())
        .ok_or_else(|| bad(format!("'{name}' has no sizes")))?;
    let (family, rest) = body.split_at(at);
    family_ok(family)?;
    if !rest.starts_with(|c: char| c.is_ascii_digit()) {
        return Err(bad(format!(
            "'{name}': the sizes follow the letters of the family"
        )));
    }
    let shape = if let Some((pitch, tail)) = rest.split_once('P') {
        let (dims, pins) = tail.split_once('-').ok_or_else(|| {
            bad(format!(
                "'{name}': a part with leads has its pins after a dash"
            ))
        })?;
        let sizes: Vec<u32> = dims
            .split('X')
            .map(|d| whole(d, &name))
            .collect::<Result<_, _>>()?;
        let pins = whole(pins, &name)?;
        if !(1..=MAX_PINS).contains(&pins) {
            return Err(bad(format!("'{name}': {pins} pins, from 1 to {MAX_PINS}")));
        }
        let mm = |v: u32| f64::from(v) / 100.0;
        match sizes.as_slice() {
            [span, height] => Shape::Dual {
                pitch_mm: mm(whole(pitch, &name)?),
                span_mm: mm(*span),
                height_mm: mm(*height),
                pins,
            },
            [l, w, h] => Shape::Quad {
                pitch_mm: mm(whole(pitch, &name)?),
                length_mm: mm(*l),
                width_mm: mm(*w),
                height_mm: mm(*h),
                pins,
            },
            _ => {
                return Err(bad(format!(
                    "'{name}': two sizes (span, height) or three (length, width, height) follow the pitch"
                )))
            }
        }
    } else {
        let (body, height) = rest
            .split_once('X')
            .ok_or_else(|| bad(format!("'{name}': a chip is written LLWWXHH")))?;
        if body.len() != 4 || !body.chars().all(|c| c.is_ascii_digit()) {
            return Err(bad(format!(
                "'{name}': the body of a chip is four digits, the length and the width in tenths of a millimetre"
            )));
        }
        let tenth = |s: &str| f64::from(whole(s, &name).unwrap_or(0)) / 10.0;
        Shape::Chip {
            length_mm: tenth(&body[..2]),
            width_mm: tenth(&body[2..]),
            height_mm: f64::from(whole(height, &name)?) / 100.0,
        }
    };
    Ok(IpcName {
        family: family.to_string(),
        shape,
        density,
    })
}

/// The letters of the family that a package of the table of packages is of, by the letter in front of its code: `R` a resistor, `C` a capacitor, `L` an inductor, `LED` a LED.
pub fn chip_family(name: &str) -> Option<&'static str> {
    let up = name.trim().to_ascii_uppercase();
    let letters: String = up.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    match letters.as_str() {
        "R" | "RES" | "RESISTOR" => Some("RESC"),
        "C" | "CAP" | "CAPACITOR" => Some("CAPC"),
        "L" | "IND" | "INDUCTOR" => Some("INDC"),
        "LED" => Some("LEDC"),
        _ => None,
    }
}

/// The name of the convention for a chip package of the table (`R_0603`, `C0402`, `LED-1206`): the body and the height are those of `pkggen`. A package that is not a chip with a letter
/// of its kind in front is an error that says so.
pub fn from_package(package: &str, density: Density) -> Result<IpcName, PnpError> {
    let family = chip_family(package).ok_or_else(|| {
        bad(format!(
            "'{package}': a chip is named with the letter of its kind in front (R, C, L or LED) and its EIA code, as in R_0603"
        ))
    })?;
    let spec = pkggen::generate(package)?;
    let is_chip = package
        .trim()
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .count()
        >= 4;
    if !is_chip {
        return Err(bad(format!("'{package}' is not a chip with an EIA code")));
    }
    Ok(IpcName {
        family: family.to_string(),
        shape: Shape::Chip {
            length_mm: spec.body_long_mm,
            width_mm: spec.body_short_mm,
            height_mm: spec.height_mm,
        },
        density,
    })
}

/// The name written the one way: the same name with its sizes read and written again, and the density changed when one is given.
pub fn normalize(text: &str, density: Option<Density>) -> Result<String, PnpError> {
    let mut parsed = parse(text)?;
    if let Some(d) = density {
        parsed.density = d;
    }
    parsed.name()
}

/// A line that says what the name is made of.
pub fn describe(n: &IpcName) -> String {
    let d = match n.density {
        Density::Least => "least",
        Density::Nominal => "nominal",
        Density::Most => "most",
    };
    match n.shape {
        Shape::Chip {
            length_mm,
            width_mm,
            height_mm,
        } => format!(
            "{} chip, body {length_mm:.1} x {width_mm:.1} mm, height {height_mm:.2} mm, {d} density",
            n.family
        ),
        Shape::Dual {
            pitch_mm,
            span_mm,
            height_mm,
            pins,
        } => format!(
            "{} with leads on two sides, pitch {pitch_mm:.2} mm, span {span_mm:.2} mm, height {height_mm:.2} mm, {pins} pins, {d} density",
            n.family
        ),
        Shape::Quad {
            pitch_mm,
            length_mm,
            width_mm,
            height_mm,
            pins,
        } => format!(
            "{} with leads on four sides, pitch {pitch_mm:.2} mm, body {length_mm:.2} x {width_mm:.2} mm, height {height_mm:.2} mm, {pins} pins, {d} density",
            n.family
        ),
    }
}
