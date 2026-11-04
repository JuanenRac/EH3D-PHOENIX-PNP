//! The tables of parts and packages: the properties of a kind of part (height, depth through the board, pick retries, discard bin, nozzle tip, rank, correction of the origin) and the way
//! they are inherited: what the part says wins over what its package says, and that wins over the defaults of the machine configuration.
//!
//! A part on a board whose kind is in neither table is not an error: it is listed as undefined, so a bill of materials with parts that are not described yet can be read and completed later.

use crate::error::{Code, PnpError};
use crate::parts::{detect_delimiter, read_records, Part};
use std::collections::{BTreeMap, HashMap};

pub const MAX_ROWS: usize = 100_000;
const PROP_COLUMNS: [&str; 8] = [
    "height_mm",
    "through_board_mm",
    "pick_retries",
    "bin",
    "nozzle_tip",
    "rank",
    "origin_dx_mm",
    "origin_dy_mm",
];

/// What a part or a package says about itself; an empty cell says nothing and the value is inherited.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PartProps {
    pub height_mm: Option<f64>,
    pub through_board_mm: Option<f64>,
    pub pick_retries: Option<u32>,
    /// 0 is the default bin; from 1 to 4 a bin of the machine.
    pub bin: Option<u8>,
    pub nozzle_tip: Option<String>,
    pub rank: Option<i32>,
    /// The distance from the origin of the footprint in the CAD file to the point where the nozzle must hold the part, for a part at 0 degrees.
    pub origin_dx_mm: Option<f64>,
    pub origin_dy_mm: Option<f64>,
}

/// The values of every property, whatever its source.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub height_mm: f64,
    pub through_board_mm: f64,
    pub pick_retries: u32,
    pub bin: u8,
    pub nozzle_tip: String,
    pub rank: i32,
    pub origin_dx_mm: f64,
    pub origin_dy_mm: f64,
}

/// The defaults of the machine configuration: the last link of the chain of inheritance.
pub type Defaults = Resolved;

impl Default for Resolved {
    fn default() -> Self {
        Resolved {
            height_mm: 1.0,
            through_board_mm: 0.0,
            pick_retries: 2,
            bin: 0,
            nozzle_tip: String::new(),
            rank: 0,
            origin_dx_mm: 0.0,
            origin_dy_mm: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Catalog {
    packages: HashMap<String, PartProps>,
    parts: HashMap<String, PartProps>,
}

fn err(message: String) -> PnpError {
    PnpError::new(Code::CatalogBadRow, message)
}

fn opt_num(cell: &str, lo: f64, hi: f64, name: &str, row: usize) -> Result<Option<f64>, PnpError> {
    if cell.is_empty() {
        return Ok(None);
    }
    let v: f64 = cell
        .parse()
        .map_err(|_| err(format!("row {row}: {name} '{cell}' is not a number")))?;
    if !v.is_finite() || v < lo || v > hi {
        return Err(err(format!(
            "row {row}: {name} {cell} is not from {lo} to {hi}"
        )));
    }
    Ok(Some(v))
}

type Row = (Vec<String>, PartProps);

fn read_table(text: &str, key_cols: &[&str], what: &str) -> Result<Vec<Row>, PnpError> {
    let text = text.trim_start_matches('\u{feff}');
    let head_line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .ok_or_else(|| err(format!("the table of {what} is empty")))?;
    let mut records = read_records(text, detect_delimiter(head_line))
        .map_err(|e| err(format!("{what}: {e}")))?
        .into_iter();
    let header: Vec<String> = records
        .next()
        .ok_or_else(|| err(format!("the table of {what} is empty")))?
        .iter()
        .map(|h| h.to_ascii_lowercase())
        .collect();
    for (i, h) in header.iter().enumerate() {
        if !key_cols.contains(&h.as_str()) && !PROP_COLUMNS.contains(&h.as_str()) {
            return Err(err(format!("{what}: the column '{h}' is not known")));
        }
        if header[..i].contains(h) {
            return Err(err(format!("{what}: the column '{h}' is written twice")));
        }
    }
    let at = |name: &str| header.iter().position(|h| h == name);
    let mut key_at = Vec::new();
    for k in key_cols {
        key_at.push(at(k).ok_or_else(|| err(format!("{what}: the header has no column '{k}'")))?);
    }
    let mut out: Vec<Row> = Vec::new();
    for (i, f) in records.enumerate() {
        let row = i + 2;
        if out.len() >= MAX_ROWS {
            return Err(PnpError::new(
                Code::CatalogBadRow,
                format!("{what}: more than {MAX_ROWS} rows"),
            ));
        }
        if f.len() != header.len() {
            return Err(err(format!(
                "{what} row {row}: {} fields, not {}",
                f.len(),
                header.len()
            )));
        }
        let cell = |name: &str| at(name).map_or("", |c| f[c].as_str());
        let keys: Vec<String> = key_at.iter().map(|&c| f[c].clone()).collect();
        if keys
            .iter()
            .any(|k| k.is_empty() || k.chars().any(char::is_control))
        {
            return Err(err(format!(
                "{what} row {row}: a name is empty or has a control character"
            )));
        }
        let tip = cell("nozzle_tip");
        if tip.chars().count() > 64 || tip.chars().any(char::is_control) {
            return Err(err(format!(
                "{what} row {row}: the nozzle tip is longer than 64 characters or has a control character"
            )));
        }
        let int = |name: &str, lo: f64, hi: f64| -> Result<Option<f64>, PnpError> {
            let v = opt_num(cell(name), lo, hi, name, row)?;
            if v.is_some_and(|x| x.fract() != 0.0) {
                return Err(err(format!(
                    "{what} row {row}: {name} must be a whole number"
                )));
            }
            Ok(v)
        };
        let props = PartProps {
            height_mm: opt_num(cell("height_mm"), 0.0, 100.0, "height_mm", row)?,
            through_board_mm: opt_num(
                cell("through_board_mm"),
                0.0,
                20.0,
                "through_board_mm",
                row,
            )?,
            pick_retries: int("pick_retries", 0.0, 10.0)?.map(|v| v as u32),
            bin: int("bin", 0.0, 4.0)?.map(|v| v as u8),
            nozzle_tip: (!tip.is_empty()).then(|| tip.to_string()),
            rank: int("rank", -1000.0, 1000.0)?.map(|v| v as i32),
            origin_dx_mm: opt_num(cell("origin_dx_mm"), -50.0, 50.0, "origin_dx_mm", row)?,
            origin_dy_mm: opt_num(cell("origin_dy_mm"), -50.0, 50.0, "origin_dy_mm", row)?,
        };
        out.push((keys, props));
    }
    Ok(out)
}

fn part_key(value: &str, package: &str) -> String {
    format!(
        "{}|{}",
        value.trim().to_ascii_lowercase(),
        package.trim().to_ascii_lowercase()
    )
}

impl Catalog {
    pub fn new() -> Catalog {
        Catalog::default()
    }

    /// Adds a table of packages: the header has `package` and any of the property columns.
    pub fn add_packages(&mut self, text: &str) -> Result<(), PnpError> {
        for (keys, props) in read_table(text, &["package"], "packages")? {
            let key = keys[0].trim().to_ascii_lowercase();
            if self.packages.insert(key, props).is_some() {
                return Err(PnpError::new(
                    Code::CatalogDuplicate,
                    format!("the package '{}' is defined twice", keys[0]),
                ));
            }
        }
        Ok(())
    }

    /// Adds a table of parts: the header has `value`, `package` and any of the property columns.
    pub fn add_parts(&mut self, text: &str) -> Result<(), PnpError> {
        for (keys, props) in read_table(text, &["value", "package"], "parts")? {
            if self
                .parts
                .insert(part_key(&keys[0], &keys[1]), props)
                .is_some()
            {
                return Err(PnpError::new(
                    Code::CatalogDuplicate,
                    format!("the part '{} / {}' is defined twice", keys[0], keys[1]),
                ));
            }
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.packages.is_empty() && self.parts.is_empty()
    }

    /// Whether the kind of a part is described, by its own row or by the row of its package.
    pub fn is_defined(&self, part: &Part) -> bool {
        self.parts.contains_key(&part.key())
            || self
                .packages
                .contains_key(&part.package.trim().to_ascii_lowercase())
    }

    /// The kinds of part on the board that no table describes, with how many parts there are of each, in alphabetical order.
    pub fn undefined(&self, parts: &[Part]) -> Vec<(String, usize)> {
        let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
        for p in parts.iter().filter(|p| !self.is_defined(p)) {
            *kinds
                .entry(format!("{} / {}", p.value.trim(), p.package.trim()))
                .or_insert(0) += 1;
        }
        kinds.into_iter().collect()
    }

    /// Every property of a part: its own row, else the row of its package, else the default.
    pub fn resolve(&self, part: &Part, d: &Defaults) -> Resolved {
        let own = self.parts.get(&part.key());
        let pkg = self.packages.get(&part.package.trim().to_ascii_lowercase());
        let tip = own
            .and_then(|p| p.nozzle_tip.clone())
            .or_else(|| pkg.and_then(|p| p.nozzle_tip.clone()));
        Resolved {
            height_mm: own
                .and_then(|p| p.height_mm)
                .or(pkg.and_then(|p| p.height_mm))
                .unwrap_or(d.height_mm),
            through_board_mm: own
                .and_then(|p| p.through_board_mm)
                .or(pkg.and_then(|p| p.through_board_mm))
                .unwrap_or(d.through_board_mm),
            pick_retries: own
                .and_then(|p| p.pick_retries)
                .or(pkg.and_then(|p| p.pick_retries))
                .unwrap_or(d.pick_retries),
            bin: own
                .and_then(|p| p.bin)
                .or(pkg.and_then(|p| p.bin))
                .unwrap_or(d.bin),
            nozzle_tip: tip.unwrap_or_else(|| d.nozzle_tip.clone()),
            rank: own
                .and_then(|p| p.rank)
                .or(pkg.and_then(|p| p.rank))
                .unwrap_or(d.rank),
            origin_dx_mm: own
                .and_then(|p| p.origin_dx_mm)
                .or(pkg.and_then(|p| p.origin_dx_mm))
                .unwrap_or(d.origin_dx_mm),
            origin_dy_mm: own
                .and_then(|p| p.origin_dy_mm)
                .or(pkg.and_then(|p| p.origin_dy_mm))
                .unwrap_or(d.origin_dy_mm),
        }
    }
}
