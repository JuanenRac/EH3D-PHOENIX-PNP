// EH3D-PHOENIX-PNP - the packages of a board checked against the library of packages.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The packages that a CAD program writes in its position file (`R_0603_1608Metric`, `Resistor_SMD:R_0603_1608Metric`, `C0805`, `SOT-23-5`) are not always the names of the table of packages
//! that is loaded (`0603`), and a part whose package has no row has no body size, no nozzle tip and no height. For each package of the board this says how it stands: *known* (the table has a
//! row of that name), *alias* (the table has a row of the same package under another name, such as `0603`), *generated* (no row, but the size can be made from the name, as
//! `package-gen` does), *similar* (no row; a name of the table that is one or two letters away, a typing mistake perhaps) or *unknown*. Nothing is changed: the answer is a list for a person.

use crate::catalog::Catalog;
use crate::parts::{csv_field, Part};
use crate::pkggen;
use std::collections::BTreeMap;

/// How a package stands against the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Known,
    /// The name of the row of the table that is the same package.
    Alias(String),
    Generated,
    /// The name of the row of the table that is a mistake away.
    Similar(String),
    Unknown,
}

impl Status {
    pub fn word(&self) -> &'static str {
        match self {
            Status::Known => "known",
            Status::Alias(_) => "alias",
            Status::Generated => "generated",
            Status::Similar(_) => "similar",
            Status::Unknown => "unknown",
        }
    }

    /// Whether a person has to look at it.
    pub fn needs_a_look(&self) -> bool {
        matches!(self, Status::Similar(_) | Status::Unknown)
    }
}

/// One package of the board.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub package: String,
    pub parts: usize,
    pub status: Status,
}

/// The name of a package without what the libraries add: the library in front of a colon, the kind of part in front of the code (`R_`, `C_`, `LED_`), and the metric size after it.
pub fn core(name: &str) -> String {
    let after = name
        .rsplit(':')
        .next()
        .unwrap_or(name)
        .trim()
        .to_lowercase();
    let tokens: Vec<&str> = after
        .split(['_', '-'])
        .filter(|t| !t.is_empty() && !t.ends_with("metric") && *t != "handsolder")
        .collect();
    let first_is_code = tokens
        .first()
        .is_some_and(|t| t.chars().any(|c| c.is_ascii_digit()));
    // a word with no digit in front of other tokens is the kind of part (R, C, LED, D); a name that is a word and a number (`SOT-23`) keeps its word
    let skip = usize::from(
        !first_is_code
            && tokens.len() > 1
            && tokens.first().is_some_and(|t| t.len() <= 3)
            && !matches!(tokens.first(), Some(&"sot" | &"sod" | &"so")),
    );
    tokens[skip..].join("-")
}

/// The kind of part a name starts with (`r` for `R_0603`), or nothing when the name starts with its code.
pub fn kind_of(name: &str) -> String {
    let after = name
        .rsplit(':')
        .next()
        .unwrap_or(name)
        .trim()
        .to_lowercase();
    let mut tokens = after.split(['_', '-']).filter(|t| !t.is_empty());
    match (tokens.next(), tokens.next()) {
        (Some(first), Some(_))
            if first.len() <= 3
                && !first.chars().any(|c| c.is_ascii_digit())
                && !matches!(first, "sot" | "sod" | "so") =>
        {
            first.to_string()
        }
        _ => String::new(),
    }
}

/// The number of single-letter changes (add, take out, change) from one text to the other.
pub fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut diag = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let up = row[j + 1];
            row[j + 1] = (diag + usize::from(ca != cb)).min(row[j] + 1).min(up + 1);
            diag = up;
        }
    }
    row[b.len()]
}

fn status_of(package: &str, catalog: &Catalog, names: &[(String, String)]) -> Status {
    if package.is_empty() {
        return Status::Unknown;
    }
    if catalog.package(package).is_some() {
        return Status::Known;
    }
    let c = core(package);
    let kind = kind_of(package);
    // several rows may be the same package of different kinds (`R_0603` and `C_0603`): the one of the same kind is the alias
    let mut same: Vec<&(String, String)> = names
        .iter()
        .filter(|(k, _)| !k.is_empty() && *k == c)
        .collect();
    same.sort_by_key(|(_, n)| (kind_of(n) != kind, n.to_lowercase()));
    if let Some((_, name)) = same.first() {
        return Status::Alias(name.clone());
    }
    if pkggen::generate(package).is_ok() || (!c.is_empty() && pkggen::generate(&c).is_ok()) {
        return Status::Generated;
    }
    let mut best: Option<((usize, bool, String), &str)> = None;
    for (k, name) in names {
        let d = distance(&c, k);
        let near = (1..=2).contains(&d) && d * 3 <= c.chars().count().min(k.chars().count());
        let rank = (d, kind_of(name) != kind, name.to_lowercase());
        if near && best.as_ref().is_none_or(|(b, _)| rank < *b) {
            best = Some((rank, name));
        }
    }
    match best {
        Some((_, name)) => Status::Similar(name.to_string()),
        None => Status::Unknown,
    }
}

/// How each package of the parts stands. The packages that most parts have come first, then by name.
pub fn check(parts: &[Part], catalog: &Catalog) -> Vec<Row> {
    let names: Vec<(String, String)> = catalog
        .package_names()
        .into_iter()
        .map(|n| (core(&n), n))
        .collect();
    let mut counts: BTreeMap<String, (String, usize)> = BTreeMap::new();
    for p in parts {
        let e = counts
            .entry(p.package.trim().to_lowercase())
            .or_insert_with(|| (p.package.trim().to_string(), 0));
        e.1 += 1;
    }
    let mut rows: Vec<Row> = counts
        .into_values()
        .map(|(package, n)| Row {
            status: status_of(&package, catalog, &names),
            package,
            parts: n,
        })
        .collect();
    rows.sort_by(|a, b| {
        b.parts
            .cmp(&a.parts)
            .then_with(|| a.package.to_lowercase().cmp(&b.package.to_lowercase()))
    });
    rows
}

/// The packages that a person has to look at.
pub fn problems(rows: &[Row]) -> usize {
    rows.iter().filter(|r| r.status.needs_a_look()).count()
}

/// The report for a person.
pub fn text(rows: &[Row]) -> String {
    let mut out = String::new();
    for r in rows {
        let package = if r.package.is_empty() {
            "(no package)"
        } else {
            &r.package
        };
        let note = match &r.status {
            Status::Alias(n) => format!(" as {n}"),
            Status::Similar(n) => format!(" to {n}?"),
            _ => String::new(),
        };
        out.push_str(&format!(
            "{package}: {} parts, {}{note}\n",
            r.parts,
            r.status.word()
        ));
    }
    out.push_str(&format!(
        "{} packages, {} to look at\n",
        rows.len(),
        problems(rows)
    ));
    out
}

/// The table as a CSV: `package,parts,status,library_name`.
pub fn csv(rows: &[Row]) -> String {
    let mut out = String::from("package,parts,status,library_name\n");
    for r in rows {
        let name = match &r.status {
            Status::Alias(n) | Status::Similar(n) => n.as_str(),
            _ => "",
        };
        out.push_str(&format!(
            "{},{},{},{}\n",
            csv_field(&r.package),
            r.parts,
            r.status.word(),
            csv_field(name)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parts::Side;

    fn part(r: &str, package: &str) -> Part {
        Part {
            reference: r.into(),
            value: "1k".into(),
            package: package.into(),
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            side: Side::Top,
        }
    }

    fn catalog() -> Catalog {
        let mut c = Catalog::new();
        c.add_packages("package,height_mm\n0603,0.5\nTQFP-48,1.0\nMyWeird-Pack,2\n")
            .unwrap();
        c
    }

    fn status(package: &str) -> Status {
        check(&[part("R1", package)], &catalog())[0].status.clone()
    }

    #[test]
    fn core_takes_off_what_the_libraries_add() {
        for (from, to) in [
            ("R_0603_1608Metric", "0603"),
            ("Resistor_SMD:R_0603_1608Metric", "0603"),
            (
                "C_0805_2012Metric_Pad1.18x1.45mm_HandSolder",
                "0805-pad1.18x1.45mm",
            ),
            ("LED_0603", "0603"),
            ("0402", "0402"),
            ("SOT-23-5", "sot-23-5"),
            ("SOD-123", "sod-123"),
            ("TQFP-48", "tqfp-48"),
            ("", ""),
        ] {
            assert_eq!(core(from), to, "{from}");
        }
    }

    #[test]
    fn the_kind_of_part_is_the_word_in_front_of_the_code() {
        for (name, kind) in [
            ("R_0603", "r"),
            ("Resistor_SMD:R_0603_1608Metric", "r"),
            ("LED-0603", "led"),
            ("0603", ""),
            ("SOT-23", ""),
            ("TQFP-48", ""),
            ("", ""),
        ] {
            assert_eq!(kind_of(name), kind, "{name}");
        }
    }

    #[test]
    fn of_two_rows_that_are_the_same_package_the_one_of_the_same_kind_is_the_alias() {
        let mut c = Catalog::new();
        c.add_packages("package,height_mm\nC_0603,0.5\nR_0603,0.5\n")
            .unwrap();
        let one = |package: &str| check(&[part("X1", package)], &c)[0].status.clone();
        assert_eq!(
            one("Resistor_SMD:R_0603_1608Metric"),
            Status::Alias("R_0603".into())
        );
        assert_eq!(
            one("Capacitor_SMD:C_0603_1608Metric"),
            Status::Alias("C_0603".into())
        );
        assert_eq!(one("R_0603x"), Status::Similar("R_0603".into()));
        // another kind falls to the first by name
        assert_eq!(one("L_0603_1608Metric"), Status::Alias("C_0603".into()));
    }

    #[test]
    fn distance_counts_the_changes() {
        assert_eq!(distance("", ""), 0);
        assert_eq!(distance("abc", "abc"), 0);
        assert_eq!(distance("abc", "abd"), 1);
        assert_eq!(distance("abc", "ab"), 1);
        assert_eq!(distance("", "abc"), 3);
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(distance("tqfp-48", "tqfp48"), 1);
    }

    #[test]
    fn a_package_of_the_table_is_known_in_any_case() {
        assert_eq!(status("0603"), Status::Known);
        assert_eq!(status("tqfp-48"), Status::Known);
        assert_eq!(status("MYWEIRD-PACK"), Status::Known);
    }

    #[test]
    fn a_library_name_for_a_package_of_the_table_is_an_alias() {
        assert_eq!(
            status("Resistor_SMD:R_0603_1608Metric"),
            Status::Alias("0603".into())
        );
        assert_eq!(status("TQFP_48"), Status::Alias("TQFP-48".into()));
    }

    #[test]
    fn a_package_with_no_row_but_a_known_size_is_generated() {
        assert_eq!(status("R_0805_2012Metric"), Status::Generated);
        assert_eq!(status("SOT-23"), Status::Generated);
    }

    #[test]
    fn a_name_a_mistake_away_is_similar_and_the_rest_unknown() {
        assert_eq!(status("TQFP-49"), Status::Similar("TQFP-48".into()));
        assert_eq!(
            status("MyWierd-Pack"),
            Status::Similar("MyWeird-Pack".into())
        );
        assert_eq!(status("Mystery_Connector_2x5"), Status::Unknown);
        assert_eq!(status(""), Status::Unknown);
        // a short name is never called similar
        assert_eq!(status("0604x"), Status::Unknown);
    }

    #[test]
    fn the_packages_are_counted_and_the_commonest_come_first() {
        let parts = vec![
            part("R1", "0603"),
            part("R2", "0603"),
            part("R3", "0603"),
            part("U1", "TQFP-48"),
            part("J1", "Mystery"),
            part("J2", "mystery"),
        ];
        let rows = check(&parts, &catalog());
        assert_eq!(
            rows.iter()
                .map(|r| (r.package.as_str(), r.parts))
                .collect::<Vec<_>>(),
            vec![("0603", 3), ("Mystery", 2), ("TQFP-48", 1)]
        );
        assert_eq!(problems(&rows), 1);
        let said = text(&rows);
        assert!(said.contains("Mystery: 2 parts, unknown"));
        assert!(said.ends_with("3 packages, 1 to look at\n"));
        let csv = csv(&rows);
        assert!(csv.starts_with("package,parts,status,library_name\n0603,3,known,\n"));
    }

    #[test]
    fn an_empty_board_has_nothing_to_look_at() {
        let rows = check(&[], &catalog());
        assert!(rows.is_empty());
        assert_eq!(problems(&rows), 0);
        assert_eq!(text(&rows), "0 packages, 0 to look at\n");
    }

    #[test]
    fn a_part_with_no_package_is_unknown_and_named_so() {
        let rows = check(&[part("X1", " ")], &catalog());
        assert_eq!(rows[0].status, Status::Unknown);
        assert!(text(&rows).starts_with("(no package): 1 parts, unknown"));
    }
}
