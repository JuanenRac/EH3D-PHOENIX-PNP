// EH3D-PHOENIX-PNP - the parts that were rejected, by reason, by package, by feeder and by reference.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! After a run an operator, or a vision check, writes down the parts that had to be rejected: a table `reference,reason[,board]` with a row for each. This joins it with the board and the
//! feeders: which reasons are the commonest and how much of the trouble they make together (the order of a Pareto chart), which packages and which feeders reject most for the parts that were
//! placed, and which references are rejected on a share of the boards that a person should look at. It reads files and prints; it has no chance in it.

use crate::error::{Code, PnpError};
use crate::feeding::{FeederSpec, Picks};
use crate::parts::{csv_field, detect_delimiter, read_records, Part};
use std::collections::{BTreeMap, BTreeSet};

/// The most rows of a table of rejects.
pub const MAX_ROWS: usize = 200_000;
/// The most boards of a run.
pub const MAX_BOARDS: u64 = 10_000_000;
const MAX_FIELD: usize = 64;

fn bad(detail: impl Into<String>) -> PnpError {
    PnpError::new(Code::ImportBad, detail.into())
}

/// One rejected part.
#[derive(Debug, Clone, PartialEq)]
pub struct Reject {
    pub reference: String,
    pub reason: String,
    /// The serial of the board, or empty.
    pub board: String,
}

/// Reads the table: a header with `reference` and `reason` (and, if wanted, `board`).
pub fn parse(text: &str) -> Result<Vec<Reject>, PnpError> {
    let text = text.trim_start_matches('\u{feff}');
    let head_line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let records = read_records(text, detect_delimiter(head_line)).map_err(bad)?;
    let Some((header, rows)) = records.split_first() else {
        return Err(bad("the table of rejects is empty"));
    };
    let header: Vec<String> = header
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();
    let column = |name: &str| header.iter().position(|h| h == name);
    let (Some(ref_at), Some(reason_at)) = (column("reference"), column("reason")) else {
        return Err(bad(
            "the table of rejects needs the columns reference and reason",
        ));
    };
    let board_at = column("board");
    if rows.len() > MAX_ROWS {
        return Err(bad(format!(
            "more than {MAX_ROWS} rows in the table of rejects"
        )));
    }
    let mut out = Vec::with_capacity(rows.len());
    for (n, row) in rows.iter().enumerate() {
        let get = |at: Option<usize>| {
            at.and_then(|i| row.get(i))
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        };
        let r = Reject {
            reference: get(Some(ref_at)),
            reason: get(Some(reason_at)),
            board: get(board_at),
        };
        let line = n + 2;
        if r.reference.is_empty() || r.reason.is_empty() {
            return Err(bad(format!(
                "row {line}: a reference and a reason are needed"
            )));
        }
        for f in [&r.reference, &r.reason, &r.board] {
            if f.chars().count() > MAX_FIELD || f.chars().any(char::is_control) {
                return Err(bad(format!(
                    "row {line}: a field is longer than {MAX_FIELD} characters or has a control character"
                )));
            }
        }
        out.push(r);
    }
    Ok(out)
}

/// A package or a feeder.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub name: String,
    /// The placements it had: its parts on a board times the boards.
    pub placed: u64,
    pub rejected: usize,
}

impl Group {
    /// The share of the placements that were rejected, 0 to 1 (more when a part is rejected again on the same board).
    pub fn rate(&self) -> f64 {
        if self.placed == 0 {
            0.0
        } else {
            self.rejected as f64 / self.placed as f64
        }
    }
}

/// A reason and its weight.
#[derive(Debug, Clone, PartialEq)]
pub struct Reason {
    pub reason: String,
    pub count: usize,
    pub share: f64,
    /// The share of this reason and all the commoner ones.
    pub cumulative: f64,
}

/// A reference rejected on some of the boards.
#[derive(Debug, Clone, PartialEq)]
pub struct Worst {
    pub reference: String,
    pub count: usize,
    /// The rejections for each board of the run.
    pub rate: f64,
}

/// What the table says.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Report {
    pub boards: u64,
    pub total: usize,
    pub reasons: Vec<Reason>,
    pub packages: Vec<Group>,
    pub feeders: Vec<Group>,
    pub references: Vec<Worst>,
    /// The references of the table that are not parts of the board.
    pub unknown: Vec<String>,
}

fn sorted_groups(map: BTreeMap<String, Group>) -> Vec<Group> {
    let mut v: Vec<Group> = map.into_values().collect();
    v.sort_by(|a, b| {
        b.rate()
            .total_cmp(&a.rate())
            .then_with(|| b.rejected.cmp(&a.rejected))
            .then_with(|| a.name.cmp(&b.name))
    });
    v
}

/// Joins the rejects with the board. `boards` is the number of boards of the run; when it is not given, it is the number of different serials in the table, or one.
pub fn analyse(
    rejects: &[Reject],
    boards: Option<u64>,
    parts: &[Part],
    feeders: &[FeederSpec],
    picks: &Picks,
) -> Result<Report, PnpError> {
    let serials: BTreeSet<&str> = rejects
        .iter()
        .map(|r| r.board.as_str())
        .filter(|b| !b.is_empty())
        .collect();
    let boards = match boards {
        Some(n) if !(1..=MAX_BOARDS).contains(&n) => {
            return Err(bad(format!(
                "the boards of the run are from 1 to {MAX_BOARDS}"
            )))
        }
        Some(n) if (n as usize) < serials.len() => {
            return Err(bad(format!(
                "the table names {} different boards and the run has {n}",
                serials.len()
            )))
        }
        Some(n) => n,
        None => (serials.len() as u64).max(1),
    };
    let by_ref: BTreeMap<&str, usize> = parts
        .iter()
        .enumerate()
        .map(|(i, p)| (p.reference.as_str(), i))
        .collect();
    let feeder_of: BTreeMap<usize, usize> =
        picks.picks.iter().map(|p| (p.part, p.feeder)).collect();
    // what each package and each feeder placed
    let mut packages: BTreeMap<String, Group> = BTreeMap::new();
    let mut by_feeder: BTreeMap<String, Group> = BTreeMap::new();
    for (i, p) in parts.iter().enumerate() {
        let name = p.package.trim().to_string();
        packages
            .entry(name.to_lowercase())
            .or_insert_with(|| Group {
                name,
                placed: 0,
                rejected: 0,
            })
            .placed += boards;
        if let Some(f) = feeder_of.get(&i) {
            let id = feeders[*f].base.id.clone();
            by_feeder
                .entry(id.clone())
                .or_insert_with(|| Group {
                    name: id,
                    placed: 0,
                    rejected: 0,
                })
                .placed += boards;
        }
    }
    let mut report = Report {
        boards,
        total: rejects.len(),
        ..Report::default()
    };
    let mut reasons: BTreeMap<String, (String, usize)> = BTreeMap::new();
    let mut per_ref: BTreeMap<&str, usize> = BTreeMap::new();
    let mut unknown: BTreeSet<String> = BTreeSet::new();
    for r in rejects {
        let e = reasons
            .entry(r.reason.to_lowercase())
            .or_insert_with(|| (r.reason.clone(), 0));
        e.1 += 1;
        match by_ref.get(r.reference.as_str()) {
            Some(i) => {
                *per_ref.entry(r.reference.as_str()).or_insert(0) += 1;
                if let Some(g) = packages.get_mut(&parts[*i].package.trim().to_lowercase()) {
                    g.rejected += 1;
                }
                if let Some(f) = feeder_of.get(i) {
                    if let Some(g) = by_feeder.get_mut(&feeders[*f].base.id) {
                        g.rejected += 1;
                    }
                }
            }
            None => {
                unknown.insert(r.reference.clone());
            }
        }
    }
    let mut list: Vec<(String, usize)> = reasons.into_values().collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let total = rejects.len().max(1) as f64;
    let mut running = 0usize;
    for (reason, count) in list {
        running += count;
        report.reasons.push(Reason {
            reason,
            count,
            share: count as f64 / total,
            cumulative: running as f64 / total,
        });
    }
    report.packages = sorted_groups(packages);
    report.feeders = sorted_groups(by_feeder);
    let mut refs: Vec<Worst> = per_ref
        .into_iter()
        .map(|(r, count)| Worst {
            reference: r.to_string(),
            count,
            rate: count as f64 / boards as f64,
        })
        .collect();
    refs.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.reference.cmp(&b.reference))
    });
    report.references = refs;
    report.unknown = unknown.into_iter().collect();
    Ok(report)
}

impl Report {
    /// The references rejected on at least `percent` per cent of the boards.
    pub fn over(&self, percent: f64) -> Vec<&Worst> {
        self.references
            .iter()
            .filter(|w| w.rate * 100.0 + 1e-9 >= percent)
            .collect()
    }

    /// The report for a person: the reasons in the order of a Pareto chart, the packages and feeders that reject most, and the references.
    pub fn text(&self) -> String {
        let mut out = format!("{} rejects on {} boards\n", self.total, self.boards);
        for r in &self.reasons {
            out.push_str(&format!(
                "reason {}: {} ({:.1}%, {:.1}% together with the commoner ones)\n",
                r.reason,
                r.count,
                r.share * 100.0,
                r.cumulative * 100.0
            ));
        }
        for (kind, groups) in [("package", &self.packages), ("feeder", &self.feeders)] {
            for g in groups.iter().filter(|g| g.rejected > 0) {
                out.push_str(&format!(
                    "{kind} {}: {} of {} placements ({:.3}%)\n",
                    g.name,
                    g.rejected,
                    g.placed,
                    g.rate() * 100.0
                ));
            }
        }
        for w in self.references.iter().take(10) {
            out.push_str(&format!(
                "reference {}: {} times ({:.1}% of the boards)\n",
                w.reference,
                w.count,
                w.rate * 100.0
            ));
        }
        if !self.unknown.is_empty() {
            out.push_str(&format!(
                "not parts of the board: {}\n",
                self.unknown.join(" ")
            ));
        }
        out
    }

    /// The table as a CSV: `kind,name,rejected,placed,percent` for the reasons (placed empty), the packages, the feeders and the references (placed is the boards).
    pub fn csv(&self) -> String {
        let mut out = String::from("kind,name,rejected,placed,percent\n");
        for r in &self.reasons {
            out.push_str(&format!(
                "reason,{},{},,{:.3}\n",
                csv_field(&r.reason),
                r.count,
                r.share * 100.0
            ));
        }
        for (kind, groups) in [("package", &self.packages), ("feeder", &self.feeders)] {
            for g in groups {
                out.push_str(&format!(
                    "{kind},{},{},{},{:.4}\n",
                    csv_field(&g.name),
                    g.rejected,
                    g.placed,
                    g.rate() * 100.0
                ));
            }
        }
        for w in &self.references {
            out.push_str(&format!(
                "reference,{},{},{},{:.3}\n",
                csv_field(&w.reference),
                w.count,
                self.boards,
                w.rate * 100.0
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feeding::{assign, from_csv};
    use crate::parts::Side;

    fn part(r: &str, package: &str, value: &str) -> Part {
        Part {
            reference: r.into(),
            value: value.into(),
            package: package.into(),
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            side: Side::Top,
        }
    }

    fn board() -> (Vec<Part>, Vec<FeederSpec>, Picks) {
        let parts = vec![
            part("R1", "0603", "10k"),
            part("R2", "0603", "10k"),
            part("C1", "0402", "100n"),
            part("U1", "TQFP-48", "MCU"),
        ];
        let feeders = from_csv(
            "id,value,package,count,x,y\nF1,10k,0603,100,0,0\nF2,100n,0402,100,10,0\nF3,MCU,TQFP-48,10,20,0\n",
        )
        .unwrap();
        let picks = assign(&parts, &feeders, None);
        (parts, feeders, picks)
    }

    fn run(text: &str, boards: Option<u64>) -> Result<Report, PnpError> {
        let (p, f, k) = board();
        analyse(&parse(text)?, boards, &p, &f, &k)
    }

    #[test]
    fn the_table_is_read_with_or_without_the_board() {
        let r = parse("reference,reason,board\nR1,tombstone,0001\nR2, bridge ,\n").unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].reason, "bridge");
        assert_eq!(r[0].board, "0001");
        assert_eq!(
            parse("reason;reference\nx;R1\n").unwrap()[0].reference,
            "R1"
        );
    }

    #[test]
    fn a_table_that_is_wrong_is_refused() {
        for text in [
            "",
            "reference\nR1\n",
            "reference,reason\n,x\n",
            "reference,reason\nR1,\n",
            "reference,reason\nR1,\u{7}\n",
        ] {
            assert!(parse(text).is_err(), "{text:?}");
        }
        let long = format!("reference,reason\nR1,{}\n", "x".repeat(65));
        assert!(parse(&long).is_err());
    }

    #[test]
    fn the_reasons_come_in_the_order_of_a_pareto_chart() {
        let r = run(
            "reference,reason\nR1,tombstone\nR2,tombstone\nC1,Tombstone\nU1,skewed\n",
            Some(10),
        )
        .unwrap();
        assert_eq!(r.total, 4);
        assert_eq!(r.reasons[0].count, 3);
        assert_eq!(r.reasons[0].reason, "tombstone");
        assert!((r.reasons[0].share - 0.75).abs() < 1e-9);
        assert!((r.reasons[0].cumulative - 0.75).abs() < 1e-9);
        assert!((r.reasons[1].cumulative - 1.0).abs() < 1e-9);
    }

    #[test]
    fn packages_and_feeders_are_counted_against_what_they_placed() {
        let r = run("reference,reason\nR1,a\nR1,b\nU1,c\n", Some(10)).unwrap();
        let tqfp = r.packages.iter().find(|g| g.name == "TQFP-48").unwrap();
        assert_eq!((tqfp.placed, tqfp.rejected), (10, 1));
        // 0603: two parts on 10 boards = 20 placements, R1 twice
        let p0603 = r.packages.iter().find(|g| g.name == "0603").unwrap();
        assert_eq!((p0603.placed, p0603.rejected), (20, 2));
        assert!((p0603.rate() - 0.1).abs() < 1e-9);
        let f1 = r.feeders.iter().find(|g| g.name == "F1").unwrap();
        assert_eq!((f1.placed, f1.rejected), (20, 2));
        assert_eq!(r.references[0].reference, "R1");
        assert!((r.references[0].rate - 0.2).abs() < 1e-9);
    }

    #[test]
    fn the_boards_are_the_serials_when_they_are_not_given() {
        let t = "reference,reason,board\nR1,a,1\nR2,a,2\nR1,a,3\n";
        assert_eq!(run(t, None).unwrap().boards, 3);
        assert_eq!(run("reference,reason\nR1,a\n", None).unwrap().boards, 1);
        assert_eq!(run(t, Some(30)).unwrap().boards, 30);
        assert!(run(t, Some(2)).is_err());
        assert!(run(t, Some(0)).is_err());
        assert!(run(t, Some(MAX_BOARDS + 1)).is_err());
    }

    #[test]
    fn a_reference_that_is_not_on_the_board_is_told() {
        let r = run("reference,reason\nX9,a\nX9,b\nR1,a\n", Some(5)).unwrap();
        assert_eq!(r.unknown, vec!["X9".to_string()]);
        assert_eq!(r.total, 3);
        assert_eq!(r.references.len(), 1);
        assert!(r.text().contains("not parts of the board: X9"));
    }

    #[test]
    fn the_references_over_a_share_of_the_boards_are_picked_out() {
        let t = "reference,reason,board\nR1,a,1\nR1,a,2\nR2,a,3\n";
        let r = run(t, Some(10)).unwrap();
        assert_eq!(r.over(20.0).len(), 1);
        assert_eq!(r.over(10.0).len(), 2);
        assert_eq!(r.over(0.0).len(), 2);
        assert!(r.over(50.0).is_empty());
    }

    #[test]
    fn the_text_and_the_table_say_the_same() {
        let r = run("reference,reason\nR1,a\nU1,b\n", Some(4)).unwrap();
        let text = r.text();
        assert!(text.starts_with("2 rejects on 4 boards\n"));
        assert!(text.contains("reason a: 1 (50.0%, 50.0% together with the commoner ones)"));
        assert!(text.contains("package 0603: 1 of 8 placements (12.500%)"));
        let csv = r.csv();
        assert!(csv.starts_with("kind,name,rejected,placed,percent\nreason,a,1,,50.000\n"));
        assert!(csv.contains("feeder,F1,1,8,12.5000\n"));
        assert!(csv.contains("reference,R1,1,4,25.000\n"));
    }

    #[test]
    fn a_run_without_rejects_is_a_clean_report() {
        let r = run("reference,reason\n", Some(3)).unwrap();
        assert_eq!(r.total, 0);
        assert!(r.reasons.is_empty() && r.references.is_empty());
        assert!(r.over(0.0).is_empty());
        assert!(r.packages.iter().all(|g| g.rejected == 0));
    }
}
