//! The position file of a board: KiCad `.pos` (text with columns) or a CSV table from any CAD program. Positions come out in millimetres.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Top,
    Bottom,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub reference: String,
    pub value: String,
    pub package: String,
    pub x: f64,
    pub y: f64,
    /// Degrees, 0 to 360.
    pub rotation: f64,
    pub side: Side,
}

impl Part {
    /// What a feeder holds: the value and the package, without regard to case.
    pub fn key(&self) -> String {
        format!(
            "{}|{}",
            self.value.trim().to_ascii_lowercase(),
            self.package.trim().to_ascii_lowercase()
        )
    }
}

pub const MAX_PARTS: usize = 50_000;
const MAX_COORDINATE: f64 = 10_000.0;

/// A number with an optional unit (`12.5`, `12.5mm`, `492mil`, `0.5in`), in millimetres; a bare number is taken as `bare` millimetres per unit (the unit the column header names).
pub(crate) fn length_with(text: &str, bare: f64) -> Result<f64, String> {
    let t = text.trim().to_ascii_lowercase();
    let (num, factor) = if let Some(n) = t.strip_suffix("mm") {
        (n, 1.0)
    } else if let Some(n) = t.strip_suffix("mil") {
        (n, 0.0254)
    } else if let Some(n) = t.strip_suffix("in") {
        (n, 25.4)
    } else {
        (t.as_str(), bare)
    };
    let v: f64 = num
        .trim()
        .parse()
        .map_err(|_| format!("'{text}' is not a length"))?;
    if !v.is_finite() || (v * factor).abs() > MAX_COORDINATE {
        return Err(format!(
            "'{text}' is not a position on a board (beyond {MAX_COORDINATE} mm)"
        ));
    }
    Ok(v * factor)
}

fn side_of(text: &str) -> Result<Side, String> {
    match text.trim().to_ascii_lowercase().as_str() {
        "top" | "t" | "front" | "f" | "toplayer" | "top layer" => Ok(Side::Top),
        "bottom" | "bot" | "b" | "back" | "bottomlayer" | "bottom layer" => Ok(Side::Bottom),
        other => Err(format!("'{other}' is not a side (top or bottom)")),
    }
}

fn angle(text: &str) -> Result<f64, String> {
    let v: f64 = text
        .trim()
        .parse()
        .map_err(|_| format!("'{text}' is not an angle"))?;
    if !v.is_finite() {
        return Err(format!("'{text}' is not an angle"));
    }
    Ok(v.rem_euclid(360.0))
}

/// The delimiter of a table, told by its header row: a semicolon when it has more semicolons than commas.
pub fn detect_delimiter(head_line: &str) -> char {
    if head_line.matches(';').count() > head_line.matches(',').count() {
        ';'
    } else {
        ','
    }
}

/// The records of a table, one reader for the parts and for the feeders: fields split by `delim`, a field in double quotes may hold the delimiter, a doubled quote and a line break,
/// an empty row is left out, and every field is trimmed. A quote that is opened and never closed is an error.
pub fn read_records(text: &str, delim: char) -> Result<Vec<Vec<String>>, String> {
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let (mut field, mut quoted) = (String::new(), false);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' && chars.peek() == Some(&'"') {
                field.push('"');
                chars.next();
            } else if c == '"' {
                quoted = false;
            } else {
                field.push(c);
            }
        } else if c == '"' && field.is_empty() {
            quoted = true;
        } else if c == delim {
            record.push(std::mem::take(&mut field).trim().to_string());
        } else if c == '\n' {
            record.push(std::mem::take(&mut field).trim().to_string());
            let row = std::mem::take(&mut record);
            if !(row.len() == 1 && row[0].is_empty()) {
                records.push(row);
            }
        } else if c != '\r' {
            field.push(c);
        }
    }
    if quoted {
        return Err("a quote is opened and never closed".into());
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field.trim().to_string());
        if !(record.len() == 1 && record[0].is_empty()) {
            records.push(record);
        }
    }
    Ok(records)
}

/// Where the names of the columns of a CSV come from: the ones every CAD program uses, plus the synonyms that the configuration adds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Profile {
    pub extra_reference: Vec<String>,
    pub extra_value: Vec<String>,
    pub extra_package: Vec<String>,
    pub extra_x: Vec<String>,
    pub extra_y: Vec<String>,
    pub extra_rotation: Vec<String>,
    pub extra_side: Vec<String>,
    /// Which pair of position columns is read when the file has both the centre of the body and the origin of the part.
    pub position: PositionColumns,
    /// What is done with the angle of the parts of the bottom side.
    pub bottom_rotation: BottomRotation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PositionColumns {
    /// The first position column of the file, whatever it is called.
    #[default]
    Any,
    /// The centre of the body (`Mid X`, `Center-X`).
    Center,
    /// The origin of the part (`Ref X`).
    Origin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BottomRotation {
    /// The angle as the file gives it.
    #[default]
    Keep,
    /// The angle turned to the other direction: 360 minus the angle.
    Mirror,
    /// The angle plus a half turn.
    HalfTurn,
}

fn names_of(base: &[&str], extra: &[String]) -> Vec<String> {
    base.iter()
        .map(|s| s.to_string())
        .chain(extra.iter().map(|s| s.trim().to_ascii_lowercase()))
        .collect()
}

fn position_names(axis: char, mode: PositionColumns, extra: &[String]) -> Vec<String> {
    let a = axis;
    let mut names = vec![
        a.to_string(),
        format!("pos{a}"),
        format!("pos {a}"),
        format!("pos-{a}"),
    ];
    let center = [
        format!("mid {a}"),
        format!("mid{a}"),
        format!("center-{a}"),
        format!("center {a}"),
        format!("center{a}"),
        format!("center-{a}(mm)"),
    ];
    let origin = [format!("ref {a}"), format!("ref{a}"), format!("ref-{a}")];
    match mode {
        PositionColumns::Any => {
            names.extend(center);
            names.extend(origin);
        }
        PositionColumns::Center => names.extend(center),
        PositionColumns::Origin => names.extend(origin),
    }
    names.extend(extra.iter().map(|s| s.trim().to_ascii_lowercase()));
    names
}

fn column(header: &[String], names: &[String]) -> Option<usize> {
    header.iter().position(|h| {
        let lc = h.trim().to_ascii_lowercase();
        names.iter().any(|n| *n == lc)
    })
}

/// Like `column`, for a position: a header may name its unit, `PosX(mil)`, `Center-X(in)` or `X [mm]`, and the unit (in millimetres per unit) is returned with the column.
fn column_unit(header: &[String], names: &[String]) -> Option<(usize, f64)> {
    header.iter().enumerate().find_map(|(i, h)| {
        let lc = h.trim().to_ascii_lowercase();
        if names.iter().any(|n| *n == lc) {
            return Some((i, 1.0));
        }
        let (open, close) = if lc.ends_with(')') {
            ('(', ')')
        } else if lc.ends_with(']') {
            ('[', ']')
        } else {
            return None;
        };
        let at = lc.rfind(open)?;
        let unit = lc[at + 1..lc.len() - close.len_utf8()].trim();
        let factor = match unit {
            "mm" => 1.0,
            "mil" | "mils" => 0.0254,
            "in" | "inch" | "inches" => 25.4,
            _ => return None,
        };
        let base = lc[..at].trim();
        names.iter().any(|n| n == base).then_some((i, factor))
    })
}

/// A CSV (comma or semicolon) with a header row. The columns are found by name: designator or ref, value or val, package or footprint, x, y, rotation or rot, side or layer.
pub fn from_csv(text: &str) -> Result<Vec<Part>, String> {
    from_csv_with(text, &Profile::default())
}

/// The same with the synonyms of a profile for the names of the columns, the pair of position columns it prefers and its rule for the angle of the bottom side.
pub fn from_csv_with(text: &str, profile: &Profile) -> Result<Vec<Part>, String> {
    let text = text.trim_start_matches('\u{feff}');
    let head_line = text
        .lines()
        .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with("//"))
        .ok_or("the file is empty")?;
    let delim = detect_delimiter(head_line);
    // a row whose first field starts with // is a comment
    let mut records = read_records(text, delim)?
        .into_iter()
        .filter(|r| !r.first().is_some_and(|f| f.starts_with("//")));
    let header = records.next().ok_or("the file is empty")?;
    let find = |names: Vec<String>, what: &str| {
        column(&header, &names).ok_or_else(|| format!("the header has no column for the {what}"))
    };
    let (c_ref, c_val, c_pkg) = (
        find(
            names_of(
                &[
                    "designator",
                    "ref",
                    "reference",
                    "refdes",
                    "ref des",
                    "ref-des",
                    "part ref",
                ],
                &profile.extra_reference,
            ),
            "reference",
        )?,
        find(
            names_of(
                &["value", "val", "comment", "part value"],
                &profile.extra_value,
            ),
            "value",
        )?,
        find(
            names_of(
                &["package", "footprint", "pattern", "decal"],
                &profile.extra_package,
            ),
            "package",
        )?,
    );
    let find_unit = |names: Vec<String>, what: &str| {
        column_unit(&header, &names)
            .ok_or_else(|| format!("the header has no column for the {what}"))
    };
    let ((c_x, ux), (c_y, uy)) = (
        find_unit(
            position_names('x', profile.position, &profile.extra_x),
            "x position",
        )?,
        find_unit(
            position_names('y', profile.position, &profile.extra_y),
            "y position",
        )?,
    );
    let c_rot = find(
        names_of(
            &["rotation", "rot", "angle", "orientation"],
            &profile.extra_rotation,
        ),
        "rotation",
    )?;
    let c_side = find(
        names_of(
            &["side", "layer", "tb", "top/bottom", "topbottom"],
            &profile.extra_side,
        ),
        "side",
    )?;
    let mut parts = Vec::new();
    for (i, f) in records.enumerate() {
        if parts.len() >= MAX_PARTS {
            return Err(format!("more than {MAX_PARTS} parts"));
        }
        let at = |c: usize| {
            f.get(c)
                .map(String::as_str)
                .ok_or_else(|| format!("row {}: it has {} fields", i + 2, f.len()))
        };
        let row = |e: String| format!("row {}: {e}", i + 2);
        let mut part = Part {
            reference: at(c_ref).map_err(row)?.to_string(),
            value: at(c_val).map_err(row)?.to_string(),
            package: at(c_pkg).map_err(row)?.to_string(),
            x: length_with(at(c_x).map_err(row)?, ux).map_err(row)?,
            y: length_with(at(c_y).map_err(row)?, uy).map_err(row)?,
            rotation: angle(at(c_rot).map_err(row)?).map_err(row)?,
            side: side_of(at(c_side).map_err(row)?).map_err(row)?,
        };
        if part.side == Side::Bottom {
            part.rotation = match profile.bottom_rotation {
                BottomRotation::Keep => part.rotation,
                BottomRotation::Mirror => (360.0 - part.rotation).rem_euclid(360.0),
                BottomRotation::HalfTurn => (part.rotation + 180.0).rem_euclid(360.0),
            };
        }
        parts.push(part);
    }
    check(&parts)?;
    Ok(parts)
}

/// A KiCad `.pos` file: `## Unit = mm` (or `inch`) and rows `Ref Val Package PosX PosY Rot Side` separated by spaces; a value with spaces is not supported by the format.
pub fn from_kicad_pos(text: &str) -> Result<Vec<Part>, String> {
    let mut inches = false;
    let mut parts = Vec::new();
    for (i, line) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(c) = t.strip_prefix('#') {
            let c = c.trim_start_matches('#').trim().to_ascii_lowercase();
            if let Some(u) = c.strip_prefix("unit") {
                inches = u.contains("inch") || u.contains("in") && !u.contains("mm");
            }
            continue;
        }
        if parts.len() >= MAX_PARTS {
            return Err(format!("more than {MAX_PARTS} parts"));
        }
        let f: Vec<&str> = t.split_whitespace().collect();
        let row = |e: String| format!("line {}: {e}", i + 1);
        if f.len() != 7 {
            return Err(row(format!(
                "{} fields, not the 7 of a KiCad position row",
                f.len()
            )));
        }
        let k = if inches { 25.4 } else { 1.0 };
        let num = |s: &str| {
            s.parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && (v * k).abs() <= MAX_COORDINATE)
                .ok_or_else(|| row(format!("'{s}' is not a position")))
        };
        parts.push(Part {
            reference: f[0].to_string(),
            value: f[1].to_string(),
            package: f[2].to_string(),
            x: num(f[3])? * k,
            y: num(f[4])? * k,
            rotation: angle(f[5]).map_err(row)?,
            side: side_of(f[6]).map_err(row)?,
        });
    }
    check(&parts)?;
    Ok(parts)
}

/// Either kind of file, told apart by its first meaningful line.
pub fn parse(text: &str) -> Result<Vec<Part>, String> {
    let first = text
        .trim_start_matches('\u{feff}')
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    if first.trim_start().starts_with('#') {
        from_kicad_pos(text)
    } else {
        from_csv(text)
    }
}

fn check(parts: &[Part]) -> Result<(), String> {
    let mut seen: std::collections::HashSet<&str> =
        std::collections::HashSet::with_capacity(parts.len());
    for (i, p) in parts.iter().enumerate() {
        if p.reference.is_empty() || p.reference.chars().any(|c| c.is_control()) {
            return Err(format!(
                "part {}: the reference is empty or has a control character",
                i + 1
            ));
        }
        if !seen.insert(p.reference.as_str()) {
            return Err(format!("the reference {} is used twice", p.reference));
        }
    }
    Ok(())
}
