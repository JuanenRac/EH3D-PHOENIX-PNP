//! The bill of materials of a board and its check against the parts of the board. A BOM is a CSV with a column of references (`R1, R2, R3`, `R1-R5` or one per row), and, if it has them,
//! the value, the package, the quantity and a part number. `compare` says which references the BOM lists that the board does not have, which parts of the board the BOM leaves out, which
//! rows give another value or package than the board, and which quantities do not add up. A part that is marked as not to be mounted may be in the BOM or not: it is never a mismatch.
//!
//! Every error carries the code `PHOENIX-PNP-057` and the number of the line.

use crate::error::{Code, PnpError};
use crate::import::{expand_references, RefItem};
use crate::parts::{detect_delimiter, read_records_numbered, Part};
use std::collections::{BTreeMap, BTreeSet};

/// The most rows of a BOM.
pub const MAX_BOM_ROWS: usize = 100_000;

const REFERENCE_NAMES: [&str; 9] = [
    "designator",
    "designators",
    "reference",
    "references",
    "refdes",
    "ref",
    "parts",
    "part",
    "refs",
];
const VALUE_NAMES: [&str; 5] = ["value", "val", "comment", "description", "name"];
const PACKAGE_NAMES: [&str; 5] = [
    "footprint",
    "package",
    "pattern",
    "case",
    "package/footprint",
];
const QUANTITY_NAMES: [&str; 4] = ["quantity", "qty", "count", "quantidade"];
const NUMBER_NAMES: [&str; 6] = [
    "part number",
    "mpn",
    "manufacturer part",
    "lcsc part #",
    "lcsc",
    "supplier part",
];

/// One row of the BOM.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BomLine {
    /// The line of the file on which the row starts.
    pub line: usize,
    pub references: Vec<String>,
    pub value: String,
    pub package: String,
    /// The quantity the row declares, if it has the column and a number in it.
    pub quantity: Option<usize>,
    pub part_number: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Bom {
    pub lines: Vec<BomLine>,
}

fn bad(line: usize, detail: impl Into<String>) -> PnpError {
    PnpError::new(Code::BomBad, format!("line {line}: {}", detail.into()))
}

fn column(header: &[String], names: &[&str]) -> Option<usize> {
    header
        .iter()
        .position(|h| names.contains(&h.trim().to_ascii_lowercase().as_str()))
}

impl Bom {
    /// Reads a BOM. The header is the first row; the columns are found by name, without regard to case, and only the references are needed.
    pub fn parse(text: &str, max_rows: usize) -> Result<Bom, PnpError> {
        let text = text.trim_start_matches('\u{feff}');
        let head = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        let records = read_records_numbered(text, detect_delimiter(head))
            .map_err(|(line, e)| bad(line, e))?;
        let mut rows = records.into_iter();
        let (head_line, header) = rows.next().ok_or_else(|| bad(1, "the file is empty"))?;
        let refs_at = column(&header, &REFERENCE_NAMES).ok_or_else(|| {
            bad(
                head_line,
                "the header has no column of references (designator, reference, refdes, ref or parts)",
            )
        })?;
        let value_at = column(&header, &VALUE_NAMES);
        let package_at = column(&header, &PACKAGE_NAMES);
        let quantity_at = column(&header, &QUANTITY_NAMES);
        let number_at = column(&header, &NUMBER_NAMES);
        let cell = |row: &[String], at: Option<usize>| -> String {
            at.and_then(|i| row.get(i)).cloned().unwrap_or_default()
        };
        let mut lines = Vec::new();
        for (line, row) in rows {
            if lines.len() >= max_rows {
                return Err(bad(line, format!("more than {max_rows} rows")));
            }
            let spec = cell(&row, Some(refs_at));
            if spec.trim().is_empty() {
                return Err(bad(line, "the row has no reference"));
            }
            let mut references = Vec::new();
            for item in expand_references(&spec).map_err(|e| bad(line, e.detail))? {
                match item {
                    RefItem::Single(r) => references.push(r),
                    RefItem::Range(list) => references.extend(list),
                }
            }
            let quantity =
                match cell(&row, quantity_at).trim() {
                    "" => None,
                    q => Some(q.parse::<usize>().map_err(|_| {
                        bad(line, format!("the quantity '{q}' is not a whole number"))
                    })?),
                };
            lines.push(BomLine {
                line,
                references,
                value: cell(&row, value_at),
                package: cell(&row, package_at),
                quantity,
                part_number: cell(&row, number_at),
            });
        }
        Ok(Bom { lines })
    }

    /// How many references the BOM lists.
    pub fn reference_count(&self) -> usize {
        self.lines.iter().map(|l| l.references.len()).sum()
    }
}

/// A value without the writing that does not change it: lower case, no spaces, `ohm` and the omega sign dropped, the micro sign written `u`.
pub fn normalize_value(text: &str) -> String {
    text.to_lowercase()
        .replace("ohms", "")
        .replace("ohm", "")
        .replace(['ω', 'Ω', ' ', '\u{a0}'], "")
        .replace(['µ', 'μ'], "u")
}

/// What the check found; every list is sorted.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BomReport {
    /// References the BOM lists that the board does not have.
    pub only_in_bom: Vec<String>,
    /// Parts to mount that no row of the BOM lists.
    pub only_on_board: Vec<String>,
    /// References listed by more than one row, or twice by one.
    pub repeated: Vec<String>,
    /// `(reference, value of the BOM, value of the board)`.
    pub value_differs: Vec<(String, String, String)>,
    /// `(reference, package of the BOM, package of the board)`.
    pub package_differs: Vec<(String, String, String)>,
    /// `(line, quantity declared, references listed)`.
    pub quantity_differs: Vec<(usize, usize, usize)>,
    /// How many references of the BOM are parts of the board that are marked as not to be mounted.
    pub listed_not_mounted: usize,
    /// How many references of the BOM are parts to mount that the board has.
    pub matched: usize,
}

impl BomReport {
    /// Whether the BOM and the board agree.
    pub fn is_clean(&self) -> bool {
        self.problems() == 0
    }

    /// How many things disagree.
    pub fn problems(&self) -> usize {
        self.only_in_bom.len()
            + self.only_on_board.len()
            + self.repeated.len()
            + self.value_differs.len()
            + self.package_differs.len()
            + self.quantity_differs.len()
    }

    /// The report as text, one finding to a line.
    pub fn text(&self) -> String {
        let mut out = String::new();
        let list = |out: &mut String, title: &str, items: &[String]| {
            if !items.is_empty() {
                out.push_str(&format!(
                    "{title} ({}): {}\n",
                    items.len(),
                    items.join(", ")
                ));
            }
        };
        list(
            &mut out,
            "in the BOM but not on the board",
            &self.only_in_bom,
        );
        list(
            &mut out,
            "on the board but not in the BOM",
            &self.only_on_board,
        );
        list(&mut out, "listed more than once", &self.repeated);
        for (r, bom, board) in &self.value_differs {
            out.push_str(&format!(
                "{r}: the value is {bom} in the BOM and {board} on the board\n"
            ));
        }
        for (r, bom, board) in &self.package_differs {
            out.push_str(&format!(
                "{r}: the package is {bom} in the BOM and {board} on the board\n"
            ));
        }
        for (line, declared, listed) in &self.quantity_differs {
            out.push_str(&format!(
                "line {line}: the quantity is {declared} and the row lists {listed} references\n"
            ));
        }
        if self.is_clean() {
            out.push_str(&format!(
                "ok: the BOM and the board agree ({} parts, {} listed that are not to be mounted)\n",
                self.matched, self.listed_not_mounted
            ));
        }
        out
    }
}

/// Checks the BOM against the parts to mount and the parts that are marked as not to be mounted.
pub fn compare(bom: &Bom, parts: &[Part], not_mounted: &[Part]) -> BomReport {
    let board: BTreeMap<&str, &Part> = parts.iter().map(|p| (p.reference.as_str(), p)).collect();
    let skipped: BTreeSet<&str> = not_mounted.iter().map(|p| p.reference.as_str()).collect();
    let mut report = BomReport::default();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut repeated: BTreeSet<&str> = BTreeSet::new();
    let mut only_in_bom: BTreeSet<&str> = BTreeSet::new();
    for line in &bom.lines {
        if let Some(q) = line.quantity {
            if q != line.references.len() {
                report
                    .quantity_differs
                    .push((line.line, q, line.references.len()));
            }
        }
        for r in &line.references {
            if !seen.insert(r.as_str()) {
                repeated.insert(r.as_str());
                continue;
            }
            if skipped.contains(r.as_str()) {
                report.listed_not_mounted += 1;
                continue;
            }
            let Some(part) = board.get(r.as_str()) else {
                only_in_bom.insert(r.as_str());
                continue;
            };
            report.matched += 1;
            if !line.value.trim().is_empty()
                && normalize_value(&line.value) != normalize_value(&part.value)
            {
                report
                    .value_differs
                    .push((r.clone(), line.value.clone(), part.value.clone()));
            }
            if !line.package.trim().is_empty()
                && !part.package.trim().is_empty()
                && !line
                    .package
                    .trim()
                    .eq_ignore_ascii_case(part.package.trim())
            {
                report.package_differs.push((
                    r.clone(),
                    line.package.clone(),
                    part.package.clone(),
                ));
            }
        }
    }
    report.only_in_bom = only_in_bom.into_iter().map(str::to_string).collect();
    report.repeated = repeated.into_iter().map(str::to_string).collect();
    report.only_on_board = board
        .keys()
        .filter(|r| !seen.contains(**r))
        .map(|r| r.to_string())
        .collect();
    report.value_differs.sort();
    report.package_differs.sort();
    report
}

/// The most boards an order can be for.
pub const MAX_ORDER_BOARDS: usize = 100_000;

/// One line of the list to order: a part that the board uses, how many for one board, and how many to buy for the boards and the spare.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderLine {
    pub value: String,
    pub package: String,
    pub part_number: String,
    pub per_board: usize,
    pub boards: usize,
    /// The parts added to the ones the boards use, for the ones that are lost in the feeder, on the floor or in a failed pick.
    pub spare: usize,
    pub total: usize,
    pub references: Vec<String>,
}

/// The parts to buy for `boards` boards: the rows of the BOM that are the same part (the same part number when they have one, else the same value and package) are added together, and
/// each line gets `spare_percent` per cent more (rounded up) or `spare_min` more, whichever is larger. A reference in `not_mounted` is not counted, and a line that ends up with no part
/// is left out.
pub fn order(
    bom: &Bom,
    boards: usize,
    spare_percent: f64,
    spare_min: usize,
    not_mounted: &[String],
) -> Result<Vec<OrderLine>, PnpError> {
    let setting = |m: String| PnpError::new(Code::PlanSetting, m);
    if !(1..=MAX_ORDER_BOARDS).contains(&boards) {
        return Err(setting(format!(
            "the number of boards must be from 1 to {MAX_ORDER_BOARDS}"
        )));
    }
    if !spare_percent.is_finite() || !(0.0..=100.0).contains(&spare_percent) {
        return Err(setting("the spare per cent must be from 0 to 100".into()));
    }
    if spare_min > 1000 {
        return Err(setting(
            "the least spare parts must be from 0 to 1000".into(),
        ));
    }
    let skipped: BTreeSet<&str> = not_mounted.iter().map(String::as_str).collect();
    let mut lines: BTreeMap<(String, String, String), OrderLine> = BTreeMap::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for row in &bom.lines {
        let refs: Vec<String> = row
            .references
            .iter()
            .filter(|r| !skipped.contains(r.as_str()) && seen.insert(r.as_str()))
            .cloned()
            .collect();
        if refs.is_empty() {
            continue;
        }
        let key = if row.part_number.trim().is_empty() {
            (
                String::new(),
                normalize_value(&row.value),
                row.package.trim().to_lowercase(),
            )
        } else {
            (
                row.part_number.trim().to_lowercase(),
                String::new(),
                String::new(),
            )
        };
        let line = lines.entry(key).or_insert_with(|| OrderLine {
            value: row.value.clone(),
            package: row.package.clone(),
            part_number: row.part_number.trim().to_string(),
            per_board: 0,
            boards,
            spare: 0,
            total: 0,
            references: Vec::new(),
        });
        line.per_board += refs.len();
        line.references.extend(refs);
    }
    let mut out: Vec<OrderLine> = lines.into_values().collect();
    for l in &mut out {
        let used = l.per_board * boards;
        let by_percent = (used as f64 * spare_percent / 100.0).ceil() as usize;
        l.spare = by_percent.max(spare_min);
        l.total = used + l.spare;
    }
    out.sort_by(|a, b| {
        (
            a.package.to_lowercase(),
            a.value.to_lowercase(),
            &a.part_number,
        )
            .cmp(&(
                b.package.to_lowercase(),
                b.value.to_lowercase(),
                &b.part_number,
            ))
    });
    Ok(out)
}

/// The list as a CSV: `value,package,part_number,per_board,boards,spare,total,references`.
pub fn order_csv(lines: &[OrderLine]) -> String {
    use crate::parts::csv_field;
    let mut out =
        String::from("value,package,part_number,per_board,boards,spare,total,references\n");
    for l in lines {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            csv_field(&l.value),
            csv_field(&l.package),
            csv_field(&l.part_number),
            l.per_board,
            l.boards,
            l.spare,
            l.total,
            csv_field(&l.references.join(" "))
        ));
    }
    out
}
