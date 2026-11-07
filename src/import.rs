//! The import of a position file from a CAD program: a generic centroid CSV whose columns are named by a map file, the text of positions of EAGLE and the `.pos` text of KiCad. Whatever the
//! format, the result is the same: parts in millimetres with a reference that no other part has, an angle from 0 up to 360, the side, and the parts that are marked as not to be mounted
//! kept apart. `Imported::to_csv` writes them as the table of parts that the rest of the program reads (`ref,val,package,x,y,rot,side,dnp`).
//!
//! The readers are the ones of this program and nothing else: there is no library of the other repositories of the suite that reads these files and could be used without making this
//! program depend on another repository, so the three readers are small and built here. The layout of the EAGLE text (name, x, y, rotation `R90` or `MR90`, value and package, in that
//! order) is the one of the position scripts of that program as it is known; it has not been compared with a file written by a real installation.
//!
//! Every error carries a stable code (`PHOENIX-PNP-053` to `PHOENIX-PNP-056`) and, for a file, the number of its line.

use crate::error::{Code, PnpError};
use crate::parts::{self, BottomRotation, Part, Profile, Side, DNP_NAMES, FITTED_NAMES, MAX_PARTS};
use std::collections::HashMap;

/// The most references that a range may stand for, and that all the ranges of one list may stand for.
pub const MAX_RANGE_REFERENCES: usize = 10_000;
/// The most names a list of a map may hold, and the most characters of a name.
const MAX_MAP_NAMES: usize = 50;
const MAX_NAME_CHARS: usize = 64;
/// The most lines that a map may ask to skip before the header.
pub const MAX_SKIP_LINES: usize = 1000;

/// The unit of the numbers of a file that gives none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImportUnit {
    #[default]
    Mm,
    Mil,
    Inch,
}

impl ImportUnit {
    pub const ALL: [ImportUnit; 3] = [ImportUnit::Mm, ImportUnit::Mil, ImportUnit::Inch];

    /// The name the configuration uses.
    pub fn name(self) -> &'static str {
        match self {
            ImportUnit::Mm => "mm",
            ImportUnit::Mil => "mil",
            ImportUnit::Inch => "inch",
        }
    }

    /// The unit a word names (`mm`, `mil`, `mils`, `inch`, `in`, `inches`), without regard to case.
    pub fn parse(text: &str) -> Option<ImportUnit> {
        match text.trim().to_ascii_lowercase().as_str() {
            "mm" => Some(ImportUnit::Mm),
            "mil" | "mils" => Some(ImportUnit::Mil),
            "inch" | "in" | "inches" => Some(ImportUnit::Inch),
            _ => None,
        }
    }

    /// Millimetres in one of the unit.
    pub fn mm_per_unit(self) -> f64 {
        match self {
            ImportUnit::Mm => 1.0,
            ImportUnit::Mil => 0.0254,
            ImportUnit::Inch => 25.4,
        }
    }
}

/// The formats of a position file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// The format is told from the first line of data.
    Auto,
    Csv,
    Eagle,
    KicadPos,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::Auto => "auto",
            Format::Csv => "csv",
            Format::Eagle => "eagle",
            Format::KicadPos => "kicad-pos",
        }
    }

    pub fn parse(text: &str) -> Option<Format> {
        match text.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Format::Auto),
            "csv" => Some(Format::Csv),
            "eagle" => Some(Format::Eagle),
            "kicad-pos" | "kicad" => Some(Format::KicadPos),
            _ => None,
        }
    }
}

/// What the configuration (`machine.cfg`) decides about an import.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// The unit of the numbers of a file that gives none.
    pub unit: ImportUnit,
    /// When above 0, the X of a part of the bottom side becomes this width minus X (a file that gives the bottom side as it is seen from below). 0 leaves X as it is.
    pub mirror_width_mm: f64,
    /// What is done with the angle of the parts of the bottom side.
    pub bottom_rotation: BottomRotation,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            unit: ImportUnit::Mm,
            mirror_width_mm: 0.0,
            bottom_rotation: BottomRotation::Keep,
        }
    }
}

/// The columns of a generic centroid CSV, as the map file names them. A list of names replaces the names that the program knows for that column; an empty list keeps them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ColumnMap {
    pub reference: Vec<String>,
    pub value: Vec<String>,
    pub package: Vec<String>,
    pub x: Vec<String>,
    pub y: Vec<String>,
    pub rotation: Vec<String>,
    pub side: Vec<String>,
    /// A column whose yes leaves the part out.
    pub dnp: Vec<String>,
    /// A column whose no leaves the part out.
    pub fitted: Vec<String>,
    /// The unit of the numbers; `None` takes the one of the settings.
    pub unit: Option<ImportUnit>,
    /// The delimiter; `None` tells it from the header.
    pub delimiter: Option<char>,
    /// Lines to skip before the header.
    pub skip_lines: usize,
    /// More words for the top side, for the bottom side, for yes and for no.
    pub top_words: Vec<String>,
    pub bottom_words: Vec<String>,
    pub yes_words: Vec<String>,
    pub no_words: Vec<String>,
}

/// The keys of a map file.
pub const MAP_KEYS: [&str; 16] = [
    "reference",
    "value",
    "package",
    "x",
    "y",
    "rotation",
    "side",
    "dnp",
    "fitted",
    "unit",
    "delimiter",
    "skip_lines",
    "top_words",
    "bottom_words",
    "yes_words",
    "no_words",
];

fn map_error(line: usize, detail: impl Into<String>) -> PnpError {
    PnpError::new(Code::ImportMap, format!("line {line}: {}", detail.into()))
}

fn word_list(line: usize, key: &str, v: &str) -> Result<Vec<String>, PnpError> {
    let items: Vec<String> = v
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    if items.len() > MAX_MAP_NAMES || items.iter().any(|s| s.chars().count() > MAX_NAME_CHARS) {
        return Err(map_error(
            line,
            format!(
                "{key} is a list of at most {MAX_MAP_NAMES} names of at most {MAX_NAME_CHARS} characters"
            ),
        ));
    }
    Ok(items)
}

impl ColumnMap {
    /// Reads a map file: `key = value` lines, `#` starts a comment, every key is optional and none may be written twice.
    pub fn parse(text: &str) -> Result<ColumnMap, PnpError> {
        let mut map = ColumnMap::default();
        let mut seen: Vec<&str> = Vec::new();
        for (i, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
            let line = i + 1;
            let t = raw.split('#').next().unwrap_or("").trim();
            if t.is_empty() {
                continue;
            }
            let (key, v) = t
                .split_once('=')
                .map(|(k, v)| (k.trim(), v.trim()))
                .ok_or_else(|| map_error(line, "a line is `key = value`"))?;
            let key_name = MAP_KEYS
                .iter()
                .find(|k| **k == key)
                .ok_or_else(|| map_error(line, format!("{key} is not a key of a map")))?;
            if seen.contains(key_name) {
                return Err(map_error(line, format!("{key} is written twice")));
            }
            seen.push(key_name);
            match key {
                "reference" => map.reference = word_list(line, key, v)?,
                "value" => map.value = word_list(line, key, v)?,
                "package" => map.package = word_list(line, key, v)?,
                "x" => map.x = word_list(line, key, v)?,
                "y" => map.y = word_list(line, key, v)?,
                "rotation" => map.rotation = word_list(line, key, v)?,
                "side" => map.side = word_list(line, key, v)?,
                "dnp" => map.dnp = word_list(line, key, v)?,
                "fitted" => map.fitted = word_list(line, key, v)?,
                "unit" => {
                    map.unit = Some(
                        ImportUnit::parse(v)
                            .ok_or_else(|| map_error(line, "unit is mm, mil or inch"))?,
                    )
                }
                "delimiter" => {
                    map.delimiter = match v.to_ascii_lowercase().as_str() {
                        "auto" => None,
                        "comma" => Some(','),
                        "semicolon" => Some(';'),
                        "tab" => Some('\t'),
                        _ => {
                            return Err(map_error(
                                line,
                                "delimiter is auto, comma, semicolon or tab",
                            ))
                        }
                    }
                }
                "skip_lines" => {
                    map.skip_lines = v
                        .parse::<usize>()
                        .ok()
                        .filter(|n| *n <= MAX_SKIP_LINES)
                        .ok_or_else(|| {
                            map_error(
                                line,
                                format!("skip_lines is a whole number from 0 to {MAX_SKIP_LINES}"),
                            )
                        })?
                }
                "top_words" => map.top_words = word_list(line, key, v)?,
                "bottom_words" => map.bottom_words = word_list(line, key, v)?,
                "yes_words" => map.yes_words = word_list(line, key, v)?,
                _ => map.no_words = word_list(line, key, v)?,
            }
        }
        Ok(map)
    }
}

/// One part of an imported file, the line it came from and whether it is not to be mounted.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub part: Part,
    pub dnp: bool,
    pub line: usize,
}

/// What an import gives: the format that was read and the parts in the order of the file.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    pub format: Format,
    pub rows: Vec<Row>,
}

/// A reference or a range of references (`R1-R5`) of a list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefItem {
    Single(String),
    Range(Vec<String>),
}

fn range_error(detail: impl Into<String>) -> PnpError {
    PnpError::new(Code::ImportRange, detail)
}

/// The digits at the end of `text` and what comes before them.
fn split_digits(text: &str) -> (&str, &str) {
    let at = text
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_ascii_digit())
        .last()
        .map_or(text.len(), |(i, _)| i);
    text.split_at(at)
}

/// A list of references separated by commas, semicolons or spaces. A range is `R1-R5` or `R1-5` (the same letters, the numbers going up; leading zeros keep their width: `R08-R10` is
/// R08, R09 and R10); an item with one dash whose ends are not both numbered (`FID-A`, `TP-1`) is a reference. All the ranges of a list stand for at most `MAX_RANGE_REFERENCES`.
pub fn expand_references(spec: &str) -> Result<Vec<RefItem>, PnpError> {
    let mut items = Vec::new();
    let mut total = 0usize;
    for word in spec
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|w| !w.is_empty())
    {
        let parts_of: Vec<&str> = word.split('-').collect();
        let range = if parts_of.len() == 2 {
            let ((lp, ln), (rp, rn)) = (split_digits(parts_of[0]), split_digits(parts_of[1]));
            (!ln.is_empty() && !rn.is_empty() && !lp.is_empty()).then_some((lp, ln, rp, rn))
        } else {
            None
        };
        let Some((prefix, from, right_prefix, to)) = range else {
            items.push(RefItem::Single(word.to_string()));
            continue;
        };
        if !right_prefix.is_empty() && right_prefix != prefix {
            return Err(range_error(format!(
                "'{word}': the two ends of a range start with the same letters"
            )));
        }
        let number = |d: &str| {
            d.parse::<u64>()
                .ok()
                .filter(|n| *n <= 1_000_000)
                .ok_or_else(|| range_error(format!("'{word}': '{d}' is too big for a range")))
        };
        let (a, b) = (number(from)?, number(to)?);
        if a > b {
            return Err(range_error(format!(
                "'{word}': the range goes down, write the smaller number first"
            )));
        }
        let count = (b - a) as usize + 1;
        total += count;
        if total > MAX_RANGE_REFERENCES {
            return Err(range_error(format!(
                "the ranges stand for more than {MAX_RANGE_REFERENCES} references"
            )));
        }
        let width = if from.starts_with('0') { from.len() } else { 0 };
        items.push(RefItem::Range(
            (a..=b).map(|n| format!("{prefix}{n:0width$}")).collect(),
        ));
    }
    Ok(items)
}

impl Imported {
    /// The parts to mount, in the order of the file.
    pub fn parts(&self) -> Vec<Part> {
        self.rows
            .iter()
            .filter(|r| !r.dnp)
            .map(|r| r.part.clone())
            .collect()
    }

    /// The parts that are not to be mounted, in the order of the file.
    pub fn dnp_parts(&self) -> Vec<Part> {
        self.rows
            .iter()
            .filter(|r| r.dnp)
            .map(|r| r.part.clone())
            .collect()
    }

    /// Marks as not to be mounted the parts of a list of references and ranges. A reference that no part has is an error with the code of the mounting (a typing mistake must not leave a
    /// part in the plan); a range may cross a gap in the numbers, but one that matches no part at all is an error too.
    pub fn apply_dnp(&mut self, items: &[RefItem]) -> Result<(), PnpError> {
        let mut index: HashMap<&str, usize> = HashMap::with_capacity(self.rows.len());
        for (i, r) in self.rows.iter().enumerate() {
            index.insert(r.part.reference.as_str(), i);
        }
        let mut marks: Vec<usize> = Vec::new();
        for item in items {
            match item {
                RefItem::Single(name) => match index.get(name.as_str()) {
                    Some(&i) => marks.push(i),
                    None => {
                        return Err(PnpError::new(
                            Code::MountingBad,
                            format!("'{name}' is not the reference of a part"),
                        ))
                    }
                },
                RefItem::Range(names) => {
                    let found: Vec<usize> = names
                        .iter()
                        .filter_map(|n| index.get(n.as_str()).copied())
                        .collect();
                    if found.is_empty() {
                        return Err(range_error(format!(
                            "the range {}-{} matches no part",
                            names.first().map_or("", String::as_str),
                            names.last().map_or("", String::as_str)
                        )));
                    }
                    marks.extend(found);
                }
            }
        }
        for i in marks {
            self.rows[i].dnp = true;
        }
        Ok(())
    }

    /// The table of parts that the rest of the program reads: the header `ref,val,package,x,y,rot,side,dnp`, millimetres, and `yes` in the last column of a part that is not to be mounted.
    pub fn to_csv(&self) -> String {
        let mut out = String::from("ref,val,package,x,y,rot,side,dnp\n");
        for r in &self.rows {
            let p = &r.part;
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                parts::csv_field(&p.reference),
                parts::csv_field(&p.value),
                parts::csv_field(&p.package),
                number_text(p.x),
                number_text(p.y),
                number_text(p.rotation),
                if p.side == Side::Top { "top" } else { "bottom" },
                if r.dnp { "yes" } else { "" }
            ));
        }
        out
    }

    /// One line that says what was read.
    pub fn summary(&self) -> String {
        let bottom = self
            .rows
            .iter()
            .filter(|r| r.part.side == Side::Bottom)
            .count();
        let dnp = self.rows.iter().filter(|r| r.dnp).count();
        format!(
            "{} parts read as {}: {} to mount, {} not to be mounted, {} on the bottom side",
            self.rows.len(),
            self.format.name(),
            self.rows.len() - dnp,
            dnp,
            bottom
        )
    }
}

/// A number with at most four decimals and no trailing zeros (a tenth of a micron is far below what a machine places).
fn number_text(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() {
        "0".to_string()
    } else {
        s.to_string()
    }
}

fn bad(line: usize, detail: impl Into<String>) -> PnpError {
    PnpError::new(Code::ImportBad, format!("line {line}: {}", detail.into()))
}

const TOP_WORDS: [&str; 7] = [
    "top",
    "t",
    "front",
    "f",
    "toplayer",
    "top layer",
    "top_layer",
];
const BOTTOM_WORDS: [&str; 7] = [
    "bottom",
    "bot",
    "b",
    "back",
    "bottomlayer",
    "bottom layer",
    "bottom_layer",
];

fn side_word(text: &str, top: &[String], bottom: &[String]) -> Option<Side> {
    let t = text.trim().to_ascii_lowercase();
    if TOP_WORDS.contains(&t.as_str()) || top.contains(&t) {
        Some(Side::Top)
    } else if BOTTOM_WORDS.contains(&t.as_str()) || bottom.contains(&t) {
        Some(Side::Bottom)
    } else {
        None
    }
}

/// An angle in degrees, from 0 up to 360 (`-90` is 270, `450` is 90); a `deg` or a degree sign after the number is accepted.
fn angle_of(text: &str) -> Option<f64> {
    let t = text.trim().to_ascii_lowercase();
    let t = t
        .strip_suffix("deg")
        .or_else(|| t.strip_suffix('°'))
        .unwrap_or(&t);
    let v: f64 = t.trim().parse().ok()?;
    v.is_finite().then(|| v.rem_euclid(360.0))
}

/// The part with the rules of the settings for the bottom side applied.
fn finish_part(mut part: Part, settings: &Settings) -> Part {
    if part.side == Side::Bottom {
        if settings.mirror_width_mm > 0.0 {
            part.x = settings.mirror_width_mm - part.x;
        }
        part.rotation = settings.bottom_rotation.apply(part.rotation);
    }
    part
}

/// Keeps the rows from growing past the limit and refuses a reference that is empty, has a control character or was used before.
struct Collector {
    rows: Vec<Row>,
    first_line: HashMap<String, usize>,
}

impl Collector {
    fn new() -> Collector {
        Collector {
            rows: Vec::new(),
            first_line: HashMap::new(),
        }
    }

    fn add(&mut self, line: usize, part: Part, dnp: bool) -> Result<(), PnpError> {
        if self.rows.len() >= MAX_PARTS {
            return Err(PnpError::new(
                Code::TooManyParts,
                format!("line {line}: more than {MAX_PARTS} parts"),
            ));
        }
        if part.reference.is_empty() || part.reference.chars().any(|c| c.is_control()) {
            return Err(bad(
                line,
                "the reference is empty or has a control character",
            ));
        }
        if let Some(first) = self.first_line.get(&part.reference) {
            return Err(bad(
                line,
                format!(
                    "the reference {} is used twice (first on line {first})",
                    part.reference
                ),
            ));
        }
        self.first_line.insert(part.reference.clone(), line);
        self.rows.push(Row { part, dnp, line });
        Ok(())
    }
}

/// The delimiter of a header row: the one of comma, semicolon and tab that it has most (a comma when there is a tie).
fn delimiter_of(head_line: &str) -> char {
    let count = |c: char| head_line.matches(c).count();
    let (commas, semicolons, tabs) = (count(','), count(';'), count('\t'));
    if tabs > commas && tabs > semicolons {
        '\t'
    } else if semicolons > commas {
        ';'
    } else {
        ','
    }
}

fn names_for(map: &[String], base: &[&str]) -> Vec<String> {
    if map.is_empty() {
        base.iter().map(|s| s.to_string()).collect()
    } else {
        map.to_vec()
    }
}

/// Reads a generic centroid CSV. The columns are found by name (the ones the program knows, or the ones of the map); the side and the mounting columns are optional.
fn read_csv(text: &str, map: &ColumnMap, settings: &Settings) -> Result<Imported, PnpError> {
    let text = text.trim_start_matches('\u{feff}');
    let head_line = text
        .lines()
        .skip(map.skip_lines)
        .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with("//"))
        .ok_or_else(|| bad(1, "the file is empty"))?;
    let delim = map.delimiter.unwrap_or_else(|| delimiter_of(head_line));
    let records = parts::read_records_numbered(text, delim).map_err(|(line, e)| bad(line, e))?;
    let mut records = records.into_iter().filter(|(line, r)| {
        *line > map.skip_lines && !r.first().is_some_and(|f| f.starts_with("//"))
    });
    let (head_no, header) = records.next().ok_or_else(|| bad(1, "the file is empty"))?;
    let missing = |what: &str, names: &[String]| {
        PnpError::new(
            Code::ImportMap,
            format!(
                "line {head_no}: the header has no column for the {what} (names looked for: {})",
                names.join(", ")
            ),
        )
    };
    let find = |what: &str, names: Vec<String>| {
        parts::column(&header, &names).ok_or_else(|| missing(what, &names))
    };
    let unit_mm = map.unit.unwrap_or(settings.unit).mm_per_unit();
    let find_xy = |what: &str, names: Vec<String>| {
        parts::column(&header, &names)
            .map(|i| (i, unit_mm))
            .or_else(|| parts::column_unit(&header, &names))
            .ok_or_else(|| missing(what, &names))
    };
    let c_ref = find(
        "reference",
        names_for(
            &map.reference,
            &[
                "designator",
                "ref",
                "reference",
                "refdes",
                "ref des",
                "ref-des",
                "part ref",
                "part",
                "name",
            ],
        ),
    )?;
    let c_val = find(
        "value",
        names_for(&map.value, &["value", "val", "comment", "part value"]),
    )?;
    let c_pkg = find(
        "package",
        names_for(&map.package, &["package", "footprint", "pattern", "decal"]),
    )?;
    let axis = |axis: char, given: &[String]| {
        if given.is_empty() {
            parts::position_names(axis, parts::PositionColumns::Any, &[])
        } else {
            given.to_vec()
        }
    };
    let (c_x, ux) = find_xy("x position", axis('x', &map.x))?;
    let (c_y, uy) = find_xy("y position", axis('y', &map.y))?;
    let c_rot = find(
        "rotation",
        names_for(&map.rotation, &["rotation", "rot", "angle", "orientation"]),
    )?;
    let side_names = names_for(
        &map.side,
        &["side", "layer", "tb", "top/bottom", "topbottom"],
    );
    let c_side = parts::column(&header, &side_names);
    let mounting: Option<(usize, bool)> = parts::column(
        &header,
        &if map.dnp.is_empty() {
            parts::names_of(&DNP_NAMES, &[])
        } else {
            map.dnp.clone()
        },
    )
    .map(|c| (c, false))
    .or_else(|| {
        parts::column(
            &header,
            &if map.fitted.is_empty() {
                parts::names_of(&FITTED_NAMES, &[])
            } else {
                map.fitted.clone()
            },
        )
        .map(|c| (c, true))
    });
    let words = Profile {
        extra_yes: map.yes_words.clone(),
        extra_no: map.no_words.clone(),
        ..Profile::default()
    };
    let mut out = Collector::new();
    for (line, f) in records {
        let cell = |c: usize, what: &str| {
            f.get(c).map(String::as_str).ok_or_else(|| {
                bad(
                    line,
                    format!(
                        "it has {} fields, the column of the {what} is the {}",
                        f.len(),
                        c + 1
                    ),
                )
            })
        };
        let length = |c: usize, factor: f64, what: &str| -> Result<f64, PnpError> {
            parts::length_with(cell(c, what)?, factor)
                .map_err(|e| bad(line, format!("{what}: {e}")))
        };
        let rotation_text = cell(c_rot, "rotation")?;
        let rotation = angle_of(rotation_text)
            .ok_or_else(|| bad(line, format!("'{rotation_text}' is not an angle")))?;
        let side = match c_side {
            Some(c) => {
                let t = cell(c, "side")?;
                side_word(t, &map.top_words, &map.bottom_words)
                    .ok_or_else(|| bad(line, format!("'{t}' is not a side (top or bottom)")))?
            }
            None => Side::Top,
        };
        let dnp = match mounting {
            Some((c, fitted)) => !parts::is_mounted(cell(c, "mounting")?, fitted, &words)
                .map_err(|e| PnpError::new(Code::MountingBad, format!("line {line}: {e}")))?,
            None => false,
        };
        let part = Part {
            reference: cell(c_ref, "reference")?.to_string(),
            value: cell(c_val, "value")?.to_string(),
            package: cell(c_pkg, "package")?.to_string(),
            x: length(c_x, ux, "x")?,
            y: length(c_y, uy, "y")?,
            rotation,
            side,
        };
        out.add(line, finish_part(part, settings), dnp)?;
    }
    Ok(Imported {
        format: Format::Csv,
        rows: out.rows,
    })
}

/// The unit that a comment of a text file declares (`## Unit = mm, Angle = deg.`, `; unit: mil`), if it declares one.
fn unit_directive(comment: &str, line: usize) -> Result<Option<ImportUnit>, PnpError> {
    let lc = comment.to_ascii_lowercase();
    let Some(at) = lc.find("unit") else {
        return Ok(None);
    };
    let rest = lc[at + 4..].trim_start();
    let Some(rest) = rest.strip_prefix('=').or_else(|| rest.strip_prefix(':')) else {
        return Ok(None);
    };
    let word: String = rest
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    ImportUnit::parse(&word)
        .map(Some)
        .ok_or_else(|| bad(line, format!("'{word}' is not a unit (mm, mil or inch)")))
}

/// Whether a line of a text file is a comment, and the text after the sign.
fn comment_of(line: &str) -> Option<&str> {
    let t = line.trim_start();
    t.strip_prefix('#')
        .or_else(|| t.strip_prefix(';'))
        .or_else(|| t.strip_prefix("//"))
}

/// Splits a line at white space; a field in double quotes may hold spaces.
fn fields_of(line: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '"' {
            chars.next();
            let mut field = String::new();
            loop {
                match chars.next() {
                    Some('"') if chars.peek() == Some(&'"') => {
                        field.push('"');
                        chars.next();
                    }
                    Some('"') => break,
                    Some(c) => field.push(c),
                    None => return Err("a quote is opened and never closed".into()),
                }
            }
            out.push(field);
        } else {
            let mut field = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                field.push(c);
                chars.next();
            }
            out.push(field);
        }
    }
    Ok(out)
}

/// The rows of a text of positions: every line that is neither empty nor a comment, as its fields; the unit that a comment declares comes with them.
fn text_rows(
    text: &str,
    settings: &Settings,
) -> Result<(ImportUnit, parts::NumberedRecords), PnpError> {
    let mut unit = settings.unit;
    let mut rows = Vec::new();
    for (i, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let line = i + 1;
        if raw.trim().is_empty() {
            continue;
        }
        if let Some(c) = comment_of(raw) {
            if let Some(u) = unit_directive(c, line)? {
                unit = u;
            }
            continue;
        }
        rows.push((line, fields_of(raw).map_err(|e| bad(line, e))?));
    }
    Ok((unit, rows))
}

/// Reads the text of positions of EAGLE: `NAME X Y ROTATION VALUE PACKAGE` on each line, the rotation as `R90` or, for a part on the bottom side, `MR90`.
fn read_eagle(text: &str, settings: &Settings) -> Result<Imported, PnpError> {
    let (unit, rows) = text_rows(text, settings)?;
    let k = unit.mm_per_unit();
    let mut out = Collector::new();
    for (line, f) in rows {
        if f.len() != 6 {
            return Err(bad(
                line,
                format!(
                    "{} fields, not the 6 of a position of EAGLE (name, x, y, rotation, value, package)",
                    f.len()
                ),
            ));
        }
        let length = |t: &str, what: &str| -> Result<f64, PnpError> {
            parts::length_with(t, k).map_err(|e| bad(line, format!("{what}: {e}")))
        };
        let upper = f[3].to_ascii_uppercase();
        let (mirrored, rest) = match upper.strip_prefix('M') {
            Some(r) => (true, r),
            None => (false, upper.as_str()),
        };
        let degrees = rest.strip_prefix('R').unwrap_or(rest);
        let rotation = angle_of(degrees).ok_or_else(|| {
            bad(
                line,
                format!("'{}' is not a rotation (R90, MR90 or 90)", f[3]),
            )
        })?;
        let part = Part {
            reference: f[0].clone(),
            value: f[4].clone(),
            package: f[5].clone(),
            x: length(&f[1], "x")?,
            y: length(&f[2], "y")?,
            rotation,
            side: if mirrored { Side::Bottom } else { Side::Top },
        };
        out.add(line, finish_part(part, settings), false)?;
    }
    Ok(Imported {
        format: Format::Eagle,
        rows: out.rows,
    })
}

/// Reads a `.pos` text of KiCad: `## Unit = mm` and the rows `Ref Val Package PosX PosY Rot Side`; a value or a package with spaces is in double quotes.
fn read_kicad_pos(text: &str, settings: &Settings) -> Result<Imported, PnpError> {
    let (unit, rows) = text_rows(text, settings)?;
    let k = unit.mm_per_unit();
    let mut out = Collector::new();
    for (line, f) in rows {
        if f.len() != 7 {
            return Err(bad(
                line,
                format!("{} fields, not the 7 of a KiCad position row", f.len()),
            ));
        }
        let length = |t: &str, what: &str| -> Result<f64, PnpError> {
            parts::length_with(t, k).map_err(|e| bad(line, format!("{what}: {e}")))
        };
        let rotation =
            angle_of(&f[5]).ok_or_else(|| bad(line, format!("'{}' is not an angle", f[5])))?;
        let side = side_word(&f[6], &[], &[])
            .ok_or_else(|| bad(line, format!("'{}' is not a side (top or bottom)", f[6])))?;
        let part = Part {
            reference: f[0].clone(),
            value: f[1].clone(),
            package: f[2].clone(),
            x: length(&f[3], "x")?,
            y: length(&f[4], "y")?,
            rotation,
            side,
        };
        out.add(line, finish_part(part, settings), false)?;
    }
    Ok(Imported {
        format: Format::KicadPos,
        rows: out.rows,
    })
}

/// Tells the format of a file from its first line of data: a delimiter (comma, semicolon or tab) is a CSV; a rotation of the form `R90` or `MR90` in the fourth field of a text is EAGLE;
/// seven fields whose last is a side are KiCad.
pub fn detect(text: &str) -> Result<Format, PnpError> {
    let Some((line, raw)) = text
        .trim_start_matches('\u{feff}')
        .lines()
        .enumerate()
        .find(|(_, l)| !l.trim().is_empty() && comment_of(l).is_none())
        .map(|(i, l)| (i + 1, l))
    else {
        return Err(PnpError::new(
            Code::ImportFormat,
            "the file has no row of data",
        ));
    };
    if raw.contains([',', ';', '\t']) {
        return Ok(Format::Csv);
    }
    let f = fields_of(raw).unwrap_or_default();
    let is_rotation = |t: &str| {
        let u = t.to_ascii_uppercase();
        let u = u.strip_prefix('M').unwrap_or(&u).to_string();
        u.strip_prefix('R')
            .is_some_and(|d| !d.is_empty() && angle_of(d).is_some())
    };
    if f.len() == 6 && is_rotation(&f[3]) {
        return Ok(Format::Eagle);
    }
    if f.len() == 7 && side_word(&f[6], &[], &[]).is_some() {
        return Ok(Format::KicadPos);
    }
    Err(PnpError::new(
        Code::ImportFormat,
        format!("line {line}: it is not a CSV, an EAGLE row nor a KiCad row; name the format"),
    ))
}

/// Reads a position file in the format given; `Format::Auto` tells it from the text. The column map is for a CSV only: with another format a map is an error.
pub fn import(
    text: &str,
    format: Format,
    map: Option<&ColumnMap>,
    settings: &Settings,
) -> Result<Imported, PnpError> {
    let format = if format == Format::Auto {
        detect(text)?
    } else {
        format
    };
    if map.is_some() && format != Format::Csv {
        return Err(PnpError::new(
            Code::ImportFormat,
            format!(
                "a column map is for a csv file, and this one is read as {}",
                format.name()
            ),
        ));
    }
    match format {
        Format::Csv | Format::Auto => {
            read_csv(text, map.unwrap_or(&ColumnMap::default()), settings)
        }
        Format::Eagle => read_eagle(text, settings),
        Format::KicadPos => read_kicad_pos(text, settings),
    }
}
