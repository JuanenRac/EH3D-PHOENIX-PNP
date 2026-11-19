// EH3D-PHOENIX-PNP - the sizes of the common packages, from their names.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A table of packages needs the size of every body, and almost every board is made of the same few packages. This writes the rows for them from their names: the chips of the EIA
//! codes (`0402`, `R0603`, `C_0805`, `LED-1206`), the small outlines (`SOT-23`, `SOT-23-5`, `SOT-223`, `SOT-89`, `SOT-323`, `SOD-123`, `SOD-323`), `SOIC-N` (narrow, or `-W` for the wide
//! body), `TSSOP-N`, `QFN-N` and `DFN-N` (with the body, as in `QFN-32-5x5`, or the usual body of that many pins), and `QFP-N`, `LQFP-N` and `TQFP-N`. The sizes are the nominal ones of the
//! standards (the body without the leads, in millimetres) and are a start: a part of another maker that is a little different is a row of the table of packages that wins over these.
//!
//! The rows are those of a table of packages (`package,height_mm,tape_mm,body_long_mm,body_short_mm`), with the name as it was written, and the width of the tape that holds such a body.
//! A name that is not known is an error with its reason (and the code `PHOENIX-PNP-010`), never a guess.

use crate::error::{Code, PnpError};

/// The size of a package.
#[derive(Debug, Clone, PartialEq)]
pub struct PackageSpec {
    /// The name as it was written.
    pub name: String,
    pub height_mm: f64,
    /// The width of the tape that holds it, in millimetres.
    pub tape_mm: f64,
    pub body_long_mm: f64,
    pub body_short_mm: f64,
}

fn bad(message: impl Into<String>) -> PnpError {
    PnpError::new(Code::CatalogBadRow, message)
}

/// The chips of the EIA codes: the code, the body (long, short) and its usual height, in millimetres.
const CHIPS: [(&str, f64, f64, f64); 10] = [
    ("01005", 0.4, 0.2, 0.13),
    ("0201", 0.6, 0.3, 0.23),
    ("0402", 1.0, 0.5, 0.35),
    ("0603", 1.6, 0.8, 0.5),
    ("0805", 2.0, 1.25, 0.5),
    ("1206", 3.2, 1.6, 0.55),
    ("1210", 3.2, 2.5, 0.55),
    ("1812", 4.5, 3.2, 0.6),
    ("2010", 5.0, 2.5, 0.6),
    ("2512", 6.3, 3.2, 0.6),
];

/// The small outlines: the name, the body and the height.
const SMALL: [(&str, f64, f64, f64); 11] = [
    ("SOT-23", 2.9, 1.3, 1.1),
    ("SOT-23-3", 2.9, 1.3, 1.1),
    ("SOT-23-5", 2.9, 1.6, 1.1),
    ("SOT-23-6", 2.9, 1.6, 1.1),
    ("SOT-223", 6.5, 3.5, 1.6),
    ("SOT-89", 4.5, 2.5, 1.5),
    ("SOT-323", 2.0, 1.25, 0.95),
    ("SC-70", 2.0, 1.25, 0.95),
    ("SOD-123", 2.7, 1.6, 1.1),
    ("SOD-323", 1.7, 1.25, 0.95),
    ("SOD-523", 1.2, 0.8, 0.6),
];

/// The body length of the SOIC and TSSOP by the number of pins.
const SOIC_LENGTH: [(u32, f64); 7] = [
    (8, 4.9),
    (14, 8.65),
    (16, 9.9),
    (18, 11.55),
    (20, 12.8),
    (24, 15.4),
    (28, 17.9),
];
const TSSOP_LENGTH: [(u32, f64); 6] = [
    (8, 3.0),
    (14, 5.0),
    (16, 5.0),
    (20, 6.5),
    (24, 7.8),
    (28, 9.7),
];

/// The usual body (a square side) of a QFN or a QFP by the number of pins.
const QFN_SIDE: [(u32, f64); 8] = [
    (16, 3.0),
    (20, 4.0),
    (24, 4.0),
    (28, 5.0),
    (32, 5.0),
    (40, 6.0),
    (48, 7.0),
    (64, 9.0),
];
const QFP_SIDE: [(u32, f64); 7] = [
    (32, 7.0),
    (44, 10.0),
    (48, 7.0),
    (64, 10.0),
    (80, 12.0),
    (100, 14.0),
    (144, 20.0),
];

/// The width of the tape that holds a body of this size: the standard widths 8, 12, 16, 24, 32 and 44 millimetres.
pub fn tape_for(long_mm: f64, short_mm: f64) -> f64 {
    let (l, s) = (long_mm.max(short_mm), long_mm.min(short_mm));
    if l <= 3.5 && s <= 3.5 {
        8.0
    } else if l <= 8.0 && s <= 5.5 {
        12.0
    } else if l <= 11.0 && s <= 7.5 {
        16.0
    } else if l <= 16.0 && s <= 12.5 {
        24.0
    } else if l <= 22.0 && s <= 20.0 {
        32.0
    } else {
        44.0
    }
}

fn spec(name: &str, long: f64, short: f64, height: f64) -> PackageSpec {
    PackageSpec {
        name: name.to_string(),
        height_mm: height,
        tape_mm: tape_for(long, short),
        body_long_mm: long.max(short),
        body_short_mm: long.min(short),
    }
}

fn pins(text: &str, name: &str) -> Result<u32, PnpError> {
    text.parse::<u32>()
        .ok()
        .filter(|n| (2..=1000).contains(n))
        .ok_or_else(|| bad(format!("'{name}': '{text}' is not a number of pins")))
}

fn lookup(table: &[(u32, f64)], n: u32, name: &str, what: &str) -> Result<f64, PnpError> {
    table
        .iter()
        .find(|(p, _)| *p == n)
        .map(|(_, v)| *v)
        .ok_or_else(|| {
            let known: Vec<String> = table.iter().map(|(p, _)| p.to_string()).collect();
            bad(format!(
                "'{name}': there is no {what} of {n} pins in the table (it has {}): give the body in the name, as in 32-5x5, or write the row by hand",
                known.join(", ")
            ))
        })
}

/// The body written in a name, `LxW` in millimetres, as in `5x5` or `4.5x3.2`.
fn body_of(text: &str, name: &str) -> Result<(f64, f64), PnpError> {
    let (a, b) = text
        .split_once(['x', 'X'])
        .ok_or_else(|| bad(format!("'{name}': '{text}' is not a body written as LxW")))?;
    let side = |t: &str| {
        t.parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && (0.2..=60.0).contains(v))
            .ok_or_else(|| bad(format!("'{name}': '{t}' is not a side from 0.2 to 60 mm")))
    };
    Ok((side(a)?, side(b)?))
}

/// The size of the package called `name` (any case; `_` and `-` are the same). A chip may carry a letter in front (`R0603`, `C_0805`, `LED-1206`).
pub fn generate(name: &str) -> Result<PackageSpec, PnpError> {
    let written = name.trim();
    if written.is_empty() || written.chars().any(char::is_control) {
        return Err(bad(
            "the name of a package is empty or has a control character",
        ));
    }
    let key = written.to_ascii_uppercase().replace('_', "-");
    // a chip: letters, an optional separator and an EIA code
    let digits_at = key.find(|c: char| c.is_ascii_digit());
    if let Some(at) = digits_at {
        let (letters, code) = key.split_at(at);
        let letters = letters.trim_end_matches('-');
        if letters.chars().all(|c| c.is_ascii_alphabetic()) && letters.len() <= 4 {
            if let Some((_, l, s, h)) = CHIPS.iter().find(|c| c.0 == code) {
                return Ok(spec(written, *l, *s, *h));
            }
        }
    }
    if let Some((_, l, s, h)) = SMALL.iter().find(|c| c.0 == key) {
        return Ok(spec(written, *l, *s, *h));
    }
    let parts: Vec<&str> = key.split('-').collect();
    match parts.as_slice() {
        ["SOIC" | "SO", n] | ["SOIC" | "SO", n, "N"] => {
            let n = pins(n, written)?;
            let len = lookup(&SOIC_LENGTH, n, written, "narrow SOIC")?;
            if n > 16 {
                return Err(bad(format!(
                    "'{written}': the SOIC of {n} pins has the wide body: write {written}-W"
                )));
            }
            Ok(spec(written, len, 3.9, 1.75))
        }
        ["SOIC" | "SO", n, "W"] => {
            let n = pins(n, written)?;
            let len = lookup(&SOIC_LENGTH, n, written, "wide SOIC")?;
            if n < 16 {
                return Err(bad(format!(
                    "'{written}': the SOIC of {n} pins has the narrow body"
                )));
            }
            Ok(spec(written, len, 7.5, 2.65))
        }
        ["TSSOP", n] => {
            let n = pins(n, written)?;
            let len = lookup(&TSSOP_LENGTH, n, written, "TSSOP")?;
            Ok(spec(written, len, 4.4, 1.1))
        }
        [kind @ ("QFN" | "DFN" | "VQFN" | "WQFN"), n] => {
            let n = pins(n, written)?;
            let side = lookup(&QFN_SIDE, n, written, kind)?;
            Ok(spec(written, side, side, 0.9))
        }
        [kind @ ("QFN" | "DFN" | "VQFN" | "WQFN"), n, body] => {
            pins(n, written)?;
            let (l, s) = body_of(body, written).map_err(|e| {
                bad(format!("{} (the {kind} takes its body as LxW)", e.detail))
            })?;
            Ok(spec(written, l, s, 0.9))
        }
        [kind @ ("QFP" | "LQFP" | "TQFP"), n] => {
            let n = pins(n, written)?;
            let side = lookup(&QFP_SIDE, n, written, kind)?;
            Ok(spec(written, side, side, if *kind == "TQFP" { 1.0 } else { 1.4 }))
        }
        [kind @ ("QFP" | "LQFP" | "TQFP"), n, body] => {
            pins(n, written)?;
            let (l, s) = body_of(body, written)?;
            Ok(spec(written, l, s, if *kind == "TQFP" { 1.0 } else { 1.4 }))
        }
        _ => Err(bad(format!(
            "'{written}' is not a package that is known (chips by EIA code, SOT, SOD, SOIC, TSSOP, QFN, DFN and QFP): write its row by hand"
        ))),
    }
}

/// The rows as a table of packages: the header `package,height_mm,tape_mm,body_long_mm,body_short_mm` and a row for each package.
pub fn table_csv(specs: &[PackageSpec]) -> String {
    let mut out = String::from("package,height_mm,tape_mm,body_long_mm,body_short_mm\n");
    let n = |v: f64| {
        let t = format!("{v:.3}");
        t.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    for s in specs {
        out.push_str(&format!(
            "{},{},{},{},{}\n",
            crate::parts::csv_field(&s.name),
            n(s.height_mm),
            n(s.tape_mm),
            n(s.body_long_mm),
            n(s.body_short_mm)
        ));
    }
    out
}
