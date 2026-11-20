// EH3D-PHOENIX-PNP - the placement list and the bill of materials in the shape that board assembly houses ask for.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A house that assembles a board takes two files: the placement list (one part on each line: the designator, the place of the middle of the part along X and along Y in two columns, the
//! layer `Top` or `Bottom` and the rotation) and the bill of materials (the part number, the designators of the row, how many, a description and the footprint). The list holds the
//! parts of both kinds, the ones that are soldered on the surface and the ones that go through the board, and every part has a line of its own: a designator is never a list. A part that is not to be
//! mounted is left out of both files or kept and marked in a column whose name is a setting (`fab_dnp_column`, `Customer Supply` by default; `fab_dnp_mode`). The words that tell a part that goes through the board from
//! the rest are a setting too (`fab_tht_words`). `check_cpl` reads a placement list back and names every line that a house would refuse. The shape is the public one of these lists;
//! it has not been sent to a house.

use crate::bom::{normalize_value, Bom};
use crate::designators;
use crate::parts::{csv_field, detect_delimiter, read_records_numbered, Part, Side};
use std::collections::{BTreeMap, HashSet};

/// The columns of the placement list.
pub const CPL_COLUMNS: [&str; 5] = ["Designator", "Mid X", "Mid Y", "Layer", "Rotation"];
/// The column that says whether a part is soldered on the surface or goes through the board (only when it is asked for).
pub const MOUNT_COLUMN: &str = "Mount";
/// The columns of the bill of materials.
pub const BOM_COLUMNS: [&str; 5] = ["MPN", "Designator", "Quantity", "Description", "Footprint"];
/// What the column of the parts that are not to be mounted holds in their lines.
pub const DNP_MARK: &str = "DNP";
/// The words that tell a part that goes through the board, when the settings give none (the default of `fab_tht_words`).
pub const DEFAULT_THT_WORDS: [&str; 11] = [
    "tht",
    "dip",
    "axial",
    "radial",
    "pinheader",
    "pinsocket",
    "through",
    "to-220",
    "to-92",
    "to-247",
    "hole",
];

/// What is done with a part that is not to be mounted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DnpMode {
    /// The part stays in the files and a column marks it.
    #[default]
    Column,
    /// The part is left out of the files.
    Omit,
}

impl DnpMode {
    pub fn parse(text: &str) -> Option<DnpMode> {
        match text.trim().to_ascii_lowercase().as_str() {
            "column" => Some(DnpMode::Column),
            "omit" => Some(DnpMode::Omit),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            DnpMode::Column => "column",
            DnpMode::Omit => "omit",
        }
    }
}

/// What the files are made with.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// The words that tell a part that goes through the board (in lower case).
    pub tht_words: Vec<String>,
    pub dnp_mode: DnpMode,
    /// The name of the column that marks a part that is not to be mounted.
    pub dnp_column: String,
    /// Whether the placement list has the column `Mount`.
    pub mount_column: bool,
    /// Only the parts of this side; `None` for both.
    pub side: Option<Side>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            tht_words: DEFAULT_THT_WORDS.iter().map(|s| s.to_string()).collect(),
            dnp_mode: DnpMode::Column,
            dnp_column: "Customer Supply".to_string(),
            mount_column: false,
            side: None,
        }
    }
}

/// Whether the text holds the word as a whole: before and after it there is the end of the text or a character that is not a letter or a digit.
fn has_word(haystack: &str, word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    let mut from = 0usize;
    while let Some(at) = haystack[from..].find(word) {
        let start = from + at;
        let end = start + word.len();
        let before = haystack[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        let after = haystack[end..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric());
        if before && after {
            return true;
        }
        // the next place to look begins one character on (the word may start inside a longer one)
        from = start + haystack[start..].chars().next().map_or(1, char::len_utf8);
        if from >= haystack.len() {
            break;
        }
    }
    false
}

/// Whether a part goes through the board: its package holds one of the words of the settings.
pub fn is_tht(part: &Part, words: &[String]) -> bool {
    let package = part.package.to_ascii_lowercase();
    words.iter().any(|w| has_word(&package, w))
}

fn layer(side: Side) -> &'static str {
    match side {
        Side::Top => "Top",
        Side::Bottom => "Bottom",
    }
}

fn turn(deg: f64) -> f64 {
    let r = deg.rem_euclid(360.0);
    if r >= 360.0 - 1e-9 {
        0.0
    } else {
        r
    }
}

/// The parts of the list with whether each is not to be mounted, in the order of the designators.
fn chosen<'a>(parts: &'a [Part], dnp: &'a [Part], o: &Options) -> Vec<(&'a Part, bool)> {
    let mut list: Vec<(&Part, bool)> = parts.iter().map(|p| (p, false)).collect();
    if o.dnp_mode == DnpMode::Column {
        list.extend(dnp.iter().map(|p| (p, true)));
    }
    list.retain(|(p, _)| o.side.is_none_or(|s| s == p.side));
    list.sort_by(|a, b| designators::natural_cmp(&a.0.reference, &b.0.reference));
    list
}

/// How many parts the files hold.
pub fn count(parts: &[Part], dnp: &[Part], o: &Options) -> usize {
    chosen(parts, dnp, o).len()
}

/// The header of the placement list for these options.
pub fn cpl_header(o: &Options) -> String {
    let mut cells: Vec<String> = CPL_COLUMNS.iter().map(|s| s.to_string()).collect();
    if o.mount_column {
        cells.push(MOUNT_COLUMN.to_string());
    }
    if o.dnp_mode == DnpMode::Column {
        cells.push(csv_field(&o.dnp_column));
    }
    cells.join(",")
}

/// The placement list: one line for each part, the places in millimetres with their unit written, the rotation from 0 up to 360 with two decimals.
pub fn cpl(parts: &[Part], dnp: &[Part], o: &Options) -> String {
    let mut out = cpl_header(o);
    out.push('\n');
    for (p, off) in chosen(parts, dnp, o) {
        let mut cells = vec![
            csv_field(&p.reference),
            format!("{:.4}mm", p.x),
            format!("{:.4}mm", p.y),
            layer(p.side).to_string(),
            format!("{:.2}", turn(p.rotation)),
        ];
        if o.mount_column {
            cells.push(
                if is_tht(p, &o.tht_words) {
                    "THT"
                } else {
                    "SMD"
                }
                .to_string(),
            );
        }
        if o.dnp_mode == DnpMode::Column {
            cells.push(if off { DNP_MARK } else { "" }.to_string());
        }
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    out
}

/// A line of a placement list that a house would refuse, with its number.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub line: usize,
    pub what: String,
}

/// A number with `mm` at its end or none.
fn mm_number(text: &str) -> Option<f64> {
    let t = text.trim();
    let t = t.strip_suffix("mm").unwrap_or(t).trim();
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Reads a placement list and names what a house would refuse: a header without the designator, the places or the layer; an empty designator; a list or a range in the cell of a
/// designator (one part on each line); a designator twice; a place that is not a number; a layer that is not `Top` or `Bottom`; a rotation that is not a number. Nothing is changed.
pub fn check_cpl(text: &str) -> Vec<Problem> {
    let text = text.trim_start_matches('\u{feff}');
    let head = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let records = match read_records_numbered(text, detect_delimiter(head)) {
        Ok(r) => r,
        Err((line, what)) => return vec![Problem { line, what }],
    };
    let mut rows = records.into_iter();
    let Some((hline, header)) = rows.next() else {
        return vec![Problem {
            line: 1,
            what: "the file is empty".into(),
        }];
    };
    let at = |name: &str| {
        header
            .iter()
            .position(|h| h.trim().eq_ignore_ascii_case(name))
    };
    let mut problems = Vec::new();
    let columns: Vec<Option<usize>> = ["Designator", "Mid X", "Mid Y", "Layer", "Rotation"]
        .iter()
        .map(|n| at(n))
        .collect();
    for (n, c) in CPL_COLUMNS.iter().zip(&columns) {
        if c.is_none() {
            problems.push(Problem {
                line: hline,
                what: format!("the header has no column {n}"),
            });
        }
    }
    let [Some(d), Some(x), Some(y), Some(l), Some(r)] = columns[..] else {
        return problems;
    };
    let mut seen = HashSet::new();
    for (line, row) in rows {
        let cell = |i: usize| row.get(i).map_or("", |s| s.trim());
        let designator = cell(d);
        if designator.is_empty() {
            problems.push(Problem {
                line,
                what: "the designator is empty".into(),
            });
        } else if !designators::is_single(designator) {
            problems.push(Problem {
                line,
                what: format!("'{designator}' is a list or a range: one designator on each line"),
            });
        } else if !seen.insert(designator.to_string()) {
            problems.push(Problem {
                line,
                what: format!("the designator {designator} is in the file twice"),
            });
        }
        for (i, name) in [(x, "Mid X"), (y, "Mid Y")] {
            if mm_number(cell(i)).is_none() {
                problems.push(Problem {
                    line,
                    what: format!("{name} is a number of millimetres"),
                });
            }
        }
        if !["top", "bottom"].contains(&cell(l).to_ascii_lowercase().as_str()) {
            problems.push(Problem {
                line,
                what: "the layer is Top or Bottom".into(),
            });
        }
        if cell(r)
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .is_none()
        {
            problems.push(Problem {
                line,
                what: "the rotation is a number of degrees".into(),
            });
        }
    }
    problems
}

/// One line of the bill of materials.
#[derive(Debug, Clone, PartialEq)]
pub struct BomRow {
    pub mpn: String,
    pub designators: Vec<String>,
    pub quantity: usize,
    pub description: String,
    pub footprint: String,
    /// Whether the parts of the row are not to be mounted.
    pub dnp: bool,
}

/// The part number of each reference, from the rows of a bill of materials that has them.
fn numbers(source: Option<&Bom>) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    if let Some(b) = source {
        for line in &b.lines {
            if line.part_number.trim().is_empty() {
                continue;
            }
            for r in &line.references {
                map.entry(r.clone())
                    .or_insert_with(|| line.part_number.trim().to_string());
            }
        }
    }
    map
}

/// The rows of the bill of materials: the parts with the same part number, value and package are one row, with their designators in the natural order and their count. The part number
/// comes from `source` (the bill of materials of the CAD program) by the designator; a part it does not name has none. The rows are in the order of their first designator.
pub fn bom_rows(parts: &[Part], dnp: &[Part], source: Option<&Bom>, o: &Options) -> Vec<BomRow> {
    let number = numbers(source);
    type Key = (bool, String, String, String);
    let mut groups: BTreeMap<Key, BomRow> = BTreeMap::new();
    for (p, off) in chosen(parts, dnp, o) {
        let mpn = number.get(&p.reference).cloned().unwrap_or_default();
        let key = (
            off,
            normalize_value(&p.value),
            p.package.trim().to_ascii_lowercase(),
            mpn.to_ascii_lowercase(),
        );
        let row = groups.entry(key).or_insert_with(|| BomRow {
            mpn,
            designators: Vec::new(),
            quantity: 0,
            description: [p.value.trim(), p.package.trim()]
                .iter()
                .filter(|s| !s.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" "),
            footprint: p.package.trim().to_string(),
            dnp: off,
        });
        row.designators.push(p.reference.clone());
        row.quantity += 1;
    }
    let mut rows: Vec<BomRow> = groups.into_values().collect();
    for r in &mut rows {
        designators::sort(&mut r.designators);
    }
    rows.sort_by(|a, b| designators::natural_cmp(&a.designators[0], &b.designators[0]));
    rows
}

/// The bill of materials as a CSV: the designators of a row are one cell with commas.
pub fn bom_csv(rows: &[BomRow], o: &Options) -> String {
    let mut out = BOM_COLUMNS.join(",");
    if o.dnp_mode == DnpMode::Column {
        out.push(',');
        out.push_str(&csv_field(&o.dnp_column));
    }
    out.push('\n');
    for r in rows {
        let mut cells = vec![
            csv_field(&r.mpn),
            csv_field(&designators::join(&r.designators, ",")),
            r.quantity.to_string(),
            csv_field(&r.description),
            csv_field(&r.footprint),
        ];
        if o.dnp_mode == DnpMode::Column {
            cells.push(if r.dnp { DNP_MARK } else { "" }.to_string());
        }
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    out
}
