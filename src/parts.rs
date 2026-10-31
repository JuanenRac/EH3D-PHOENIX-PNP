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

/// A number with an optional unit (`12.5`, `12.5mm`, `492mil`, `0.5in`), in millimetres; `default_mil` makes a bare number mils.
fn length(text: &str, default_mil: bool) -> Result<f64, String> {
    let t = text.trim().to_ascii_lowercase();
    let (num, factor) = if let Some(n) = t.strip_suffix("mm") {
        (n, 1.0)
    } else if let Some(n) = t.strip_suffix("mil") {
        (n, 0.0254)
    } else if let Some(n) = t.strip_suffix("in") {
        (n, 25.4)
    } else {
        (t.as_str(), if default_mil { 0.0254 } else { 1.0 })
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

fn split_csv(line: &str, delim: char) -> Vec<String> {
    let mut out = Vec::new();
    let (mut field, mut quoted) = (String::new(), false);
    let mut chars = line.chars().peekable();
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
            out.push(std::mem::take(&mut field));
        } else {
            field.push(c);
        }
    }
    out.push(field);
    out.into_iter().map(|f| f.trim().to_string()).collect()
}

fn column(header: &[String], names: &[&str]) -> Option<usize> {
    header
        .iter()
        .position(|h| names.contains(&h.trim().to_ascii_lowercase().as_str()))
}

/// A CSV (comma or semicolon) with a header row. The columns are found by name: designator or ref, value or val, package or footprint, x, y, rotation or rot, side or layer.
pub fn from_csv(text: &str) -> Result<Vec<Part>, String> {
    let mut lines = text
        .trim_start_matches('\u{feff}')
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with("//"));
    let head_line = lines.next().ok_or("the file is empty")?;
    let delim = if head_line.matches(';').count() > head_line.matches(',').count() {
        ';'
    } else {
        ','
    };
    let header = split_csv(head_line, delim);
    let find = |names: &[&str], what: &str| {
        column(&header, names).ok_or_else(|| format!("the header has no column for the {what}"))
    };
    let (c_ref, c_val, c_pkg) = (
        find(&["designator", "ref", "reference", "refdes"], "reference")?,
        find(&["value", "val", "comment"], "value")?,
        find(&["package", "footprint", "pattern"], "package")?,
    );
    let (c_x, c_y) = (
        find(
            &["x", "posx", "mid x", "midx", "center-x(mm)", "ref x"],
            "x position",
        )?,
        find(
            &["y", "posy", "mid y", "midy", "center-y(mm)", "ref y"],
            "y position",
        )?,
    );
    let c_rot = find(&["rotation", "rot", "angle"], "rotation")?;
    let c_side = find(&["side", "layer", "tb"], "side")?;
    let mut parts = Vec::new();
    for (i, line) in lines.enumerate() {
        if parts.len() >= MAX_PARTS {
            return Err(format!("more than {MAX_PARTS} parts"));
        }
        let f = split_csv(line, delim);
        let at = |c: usize| {
            f.get(c)
                .map(String::as_str)
                .ok_or_else(|| format!("row {}: it has {} fields", i + 2, f.len()))
        };
        let row = |e: String| format!("row {}: {e}", i + 2);
        parts.push(Part {
            reference: at(c_ref).map_err(row)?.to_string(),
            value: at(c_val).map_err(row)?.to_string(),
            package: at(c_pkg).map_err(row)?.to_string(),
            x: length(at(c_x).map_err(row)?, false).map_err(row)?,
            y: length(at(c_y).map_err(row)?, false).map_err(row)?,
            rotation: angle(at(c_rot).map_err(row)?).map_err(row)?,
            side: side_of(at(c_side).map_err(row)?).map_err(row)?,
        });
    }
    check(&parts)?;
    Ok(parts)
}

/// A KiCad `.pos` file: `## Unit = mm` (or `inch`) and rows `Ref Val Package PosX PosY Rot Side` separated by spaces; a value with spaces is not supported by the format.
pub fn from_kicad_pos(text: &str) -> Result<Vec<Part>, String> {
    let mut inches = false;
    let mut parts = Vec::new();
    for (i, line) in text.lines().enumerate() {
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
    for (i, p) in parts.iter().enumerate() {
        if p.reference.is_empty() || p.reference.chars().any(|c| c.is_control()) {
            return Err(format!(
                "part {}: the reference is empty or has a control character",
                i + 1
            ));
        }
        if parts[..i].iter().any(|q| q.reference == p.reference) {
            return Err(format!("the reference {} is used twice", p.reference));
        }
    }
    Ok(())
}
