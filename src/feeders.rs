//! The feeders of the machine: each holds one kind of part at a known place, and runs out.

use crate::parts::{detect_delimiter, read_records, Part, Side};

#[derive(Debug, Clone, PartialEq)]
pub struct Feeder {
    pub id: String,
    pub value: String,
    pub package: String,
    /// Where the next part of the tape is picked, in millimetres.
    pub x: f64,
    pub y: f64,
    /// How many parts are left in the tape.
    pub count: u32,
}

impl Feeder {
    pub fn key(&self) -> String {
        format!(
            "{}|{}",
            self.value.trim().to_ascii_lowercase(),
            self.package.trim().to_ascii_lowercase()
        )
    }
}

/// A CSV of feeders with the header `id,value,package,x,y,count`.
pub fn from_csv(text: &str) -> Result<Vec<Feeder>, String> {
    let text = text.trim_start_matches('\u{feff}');
    let head_line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .ok_or("the file of feeders is empty")?;
    // the same reader as the position file: a semicolon as delimiter, quoted fields with a delimiter or a line break
    let mut records = read_records(text, detect_delimiter(head_line))?.into_iter();
    let head: Vec<String> = records
        .next()
        .ok_or("the file of feeders is empty")?
        .iter()
        .map(|h| h.to_ascii_lowercase())
        .collect();
    if head != ["id", "value", "package", "x", "y", "count"] {
        return Err(
            "the first row of the feeders must be exactly: id,value,package,x,y,count".into(),
        );
    }
    let mut out: Vec<Feeder> = Vec::new();
    let mut ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (i, record) in records.enumerate() {
        let f: Vec<&str> = record.iter().map(String::as_str).collect();
        let row = |e: String| format!("feeder row {}: {e}", i + 2);
        if f.len() != 6 {
            return Err(row(format!("{} fields, not 6", f.len())));
        }
        let num = |s: &str| {
            s.parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && v.abs() <= 10_000.0)
                .ok_or_else(|| row(format!("'{s}' is not a position")))
        };
        let feeder = Feeder {
            id: f[0].to_string(),
            value: f[1].to_string(),
            package: f[2].to_string(),
            x: num(f[3])?,
            y: num(f[4])?,
            count: f[5]
                .parse()
                .map_err(|_| row(format!("'{}' is not a count of parts", f[5])))?,
        };
        if feeder.id.is_empty() || !ids.insert(feeder.id.clone()) {
            return Err(row(format!(
                "the id '{}' is empty or used twice",
                feeder.id
            )));
        }
        out.push(feeder);
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    /// The number of the part in the list given.
    pub part: usize,
    /// The number of the feeder in the list given.
    pub feeder: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Assigned {
    pub assignments: Vec<Assignment>,
    /// Parts for which no feeder holds the value and the package, with the reference.
    pub without_feeder: Vec<String>,
    /// Feeders that do not have enough parts left, with how many are missing.
    pub short: Vec<(String, u32)>,
}

/// Gives every part the first feeder that holds its value and package and still has parts; a feeder that runs out is reported with the number of parts that are missing and the next
/// feeder of the same kind (if there is one) takes the rest.
pub fn assign(parts: &[Part], feeders: &[Feeder]) -> Assigned {
    assign_side(parts, feeders, None)
}

/// The same for one side of the board: the parts of the other side are not given a feeder, are not reported and do not use up the feeders. The numbers in the assignments
/// stay the numbers of the parts in the whole list.
pub fn assign_side(parts: &[Part], feeders: &[Feeder], side: Option<Side>) -> Assigned {
    let mut left: Vec<u32> = feeders.iter().map(|f| f.count).collect();
    let mut out = Assigned::default();
    // the feeders that hold each kind of part, in the order of the list: found once, not by looking at every feeder for every part
    let mut by_key: std::collections::HashMap<String, Vec<usize>> =
        std::collections::HashMap::new();
    for (n, f) in feeders.iter().enumerate() {
        by_key.entry(f.key()).or_default().push(n);
    }
    for (i, p) in parts.iter().enumerate() {
        if side.is_some_and(|s| s != p.side) {
            continue;
        }
        let key = p.key();
        let same: &[usize] = by_key.get(&key).map_or(&[][..], Vec::as_slice);
        if same.is_empty() {
            out.without_feeder.push(p.reference.clone());
            continue;
        }
        match same.iter().find(|&&n| left[n] > 0) {
            Some(&n) => {
                left[n] -= 1;
                out.assignments.push(Assignment { part: i, feeder: n });
            }
            None => {
                let last = *same.last().unwrap_or(&0);
                match out.short.iter_mut().find(|(id, _)| *id == feeders[last].id) {
                    Some(s) => s.1 += 1,
                    None => out.short.push((feeders[last].id.clone(), 1)),
                }
            }
        }
    }
    out
}
