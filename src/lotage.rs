// EH3D-PHOENIX-PNP - how old the reels in the feeders are, from the date codes of their lots.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! Solder-wettability and moisture limits are given in time from the date a part was made, and the reel says when: the date code `YYWW` (two digits of the year, two of the week; `YYYYWW` is
//! read too). With the table of lots that `lot-trace` reads and the week that is asked about (`YYWW`), this says for each feeder that gives parts to the board how many weeks old its lot is
//! and whether that is over the limit of the settings. The week is given to the program, never read from a clock, so the same files and the same week give the same answer.

use crate::feeding::{FeederSpec, Picks};
use crate::lottrace::Lot;
use crate::parts::csv_field;
use std::collections::BTreeSet;

/// A year and a week of the year (1 to 53).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Week {
    pub year: u32,
    pub week: u32,
}

impl Week {
    /// The weeks counted from year 2000, taking every year as 52 weeks (the age is for comparing with a limit of months, not for a calendar).
    pub fn count(self) -> i64 {
        i64::from(self.year.saturating_sub(2000)) * 52 + i64::from(self.week)
    }
}

/// Reads `YYWW` or `YYYYWW`; the week is from 1 to 53 and the year from 2000.
pub fn parse_week(text: &str) -> Option<Week> {
    let t = text.trim();
    if !t.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (year, week) = match t.len() {
        4 => (
            2000 + t[..2].parse::<u32>().ok()?,
            t[2..].parse::<u32>().ok()?,
        ),
        6 => (t[..4].parse::<u32>().ok()?, t[4..].parse::<u32>().ok()?),
        _ => return None,
    };
    ((2000..=2099).contains(&year) && (1..=53).contains(&week)).then_some(Week { year, week })
}

/// How a lot stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    /// Older than the limit.
    Old,
    /// Made after the week that was asked about: a typing mistake in the code or in the week.
    Future,
    /// The lot has no date code.
    NoCode,
    /// The date code is not `YYWW` or `YYYYWW`.
    BadCode,
    /// The feeder has no row in the table of lots.
    NoLot,
}

impl Status {
    pub fn word(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Old => "old",
            Status::Future => "future",
            Status::NoCode => "no date code",
            Status::BadCode => "bad date code",
            Status::NoLot => "no lot",
        }
    }
}

/// One feeder that gives parts.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub feeder: String,
    pub lot: String,
    pub date_code: String,
    /// The weeks between the date code and the week asked about (negative for the future); `None` when there is no readable code.
    pub weeks: Option<i64>,
    pub status: Status,
}

/// Judges the lots of the feeders that the picks use, in the order of the feeders.
pub fn check(
    feeders: &[FeederSpec],
    picks: &Picks,
    lots: &[Lot],
    now: Week,
    max_weeks: u32,
) -> Vec<Row> {
    let used: BTreeSet<usize> = picks.picks.iter().map(|p| p.feeder).collect();
    used.into_iter()
        .map(|n| {
            let id = feeders[n].base.id.clone();
            let Some(lot) = lots.iter().find(|l| l.feeder == id) else {
                return Row {
                    feeder: id,
                    lot: String::new(),
                    date_code: String::new(),
                    weeks: None,
                    status: Status::NoLot,
                };
            };
            let code = lot.date_code.trim();
            let (weeks, status) = if code.is_empty() {
                (None, Status::NoCode)
            } else if let Some(w) = parse_week(code) {
                let age = now.count() - w.count();
                let status = if age < 0 {
                    Status::Future
                } else if age > i64::from(max_weeks) {
                    Status::Old
                } else {
                    Status::Ok
                };
                (Some(age), status)
            } else {
                (None, Status::BadCode)
            };
            Row {
                feeder: id,
                lot: lot.lot.clone(),
                date_code: code.to_string(),
                weeks,
                status,
            }
        })
        .collect()
}

/// The feeders that a person has to look at.
pub fn problems(rows: &[Row]) -> usize {
    rows.iter().filter(|r| r.status != Status::Ok).count()
}

/// The report for a person.
pub fn text(rows: &[Row], max_weeks: u32) -> String {
    let mut out = String::new();
    for r in rows {
        let age = r
            .weeks
            .map_or(String::new(), |w| format!(", {w} weeks old"));
        out.push_str(&format!(
            "feeder {}: lot {} date code {}{age}: {}\n",
            r.feeder,
            if r.lot.is_empty() { "-" } else { &r.lot },
            if r.date_code.is_empty() {
                "-"
            } else {
                &r.date_code
            },
            r.status.word()
        ));
    }
    out.push_str(&format!(
        "{} feeders, {} to look at (the limit is {max_weeks} weeks)\n",
        rows.len(),
        problems(rows)
    ));
    out
}

/// The table as a CSV: `feeder,lot,date_code,weeks,status`.
pub fn csv(rows: &[Row]) -> String {
    let mut out = String::from("feeder,lot,date_code,weeks,status\n");
    for r in rows {
        out.push_str(&format!(
            "{},{},{},{},{}\n",
            csv_field(&r.feeder),
            csv_field(&r.lot),
            csv_field(&r.date_code),
            r.weeks.map_or(String::new(), |w| w.to_string()),
            r.status.word()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feeding::{assign, from_csv};
    use crate::lottrace::parse_lots;
    use crate::parts::{Part, Side};

    fn part(r: &str, value: &str, package: &str) -> Part {
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

    fn rows(lots: &str, now: &str, max: u32) -> Vec<Row> {
        let parts = vec![
            part("R1", "10k", "0603"),
            part("C1", "100n", "0402"),
            part("U1", "MCU", "TQFP-48"),
        ];
        let feeders = from_csv(
            "id,value,package,count,x,y\nF1,10k,0603,100,0,0\nF2,100n,0402,100,10,0\nF3,MCU,TQFP-48,10,20,0\nF4,1M,0603,10,30,0\n",
        )
        .unwrap();
        let picks = assign(&parts, &feeders, None);
        check(
            &feeders,
            &picks,
            &parse_lots(lots).unwrap(),
            parse_week(now).unwrap(),
            max,
        )
    }

    #[test]
    fn a_date_code_is_read_in_both_shapes_and_nothing_else() {
        assert_eq!(
            parse_week("2315"),
            Some(Week {
                year: 2023,
                week: 15
            })
        );
        assert_eq!(
            parse_week("202315"),
            Some(Week {
                year: 2023,
                week: 15
            })
        );
        assert_eq!(
            parse_week(" 0001 "),
            Some(Week {
                year: 2000,
                week: 1
            })
        );
        for bad in [
            "", "231", "23150", "2300", "2354", "1999 1", "ab12", "1999 01", "199901", "210001",
            "23-15",
        ] {
            assert_eq!(parse_week(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_weeks_are_counted_from_the_codes() {
        let a = parse_week("2301").unwrap();
        let b = parse_week("2401").unwrap();
        assert_eq!(b.count() - a.count(), 52);
        assert_eq!(parse_week("2310").unwrap().count() - a.count(), 9);
    }

    #[test]
    fn a_lot_is_ok_old_or_from_the_future() {
        let r = rows(
            "feeder,lot,date_code\nF1,L1,2301\nF2,L2,2001\nF3,L3,2601\n",
            "2410",
            104,
        );
        assert_eq!(r.len(), 3);
        assert_eq!((r[0].weeks, r[0].status), (Some(61), Status::Ok));
        assert_eq!((r[1].weeks, r[1].status), (Some(217), Status::Old));
        assert_eq!(r[2].status, Status::Future);
        assert_eq!(r[2].weeks, Some(-95));
        assert_eq!(problems(&r), 2);
    }

    #[test]
    fn the_limit_is_kept_to_the_week() {
        // exactly 104 weeks old is still ok, 105 is old
        let ok = rows(
            "feeder,lot,date_code\nF1,L,2201\nF2,L,2201\nF3,L,2201\n",
            "2401",
            104,
        );
        assert!(ok.iter().all(|r| r.status == Status::Ok));
        let old = rows(
            "feeder,lot,date_code\nF1,L,2201\nF2,L,2201\nF3,L,2201\n",
            "2402",
            104,
        );
        assert!(old.iter().all(|r| r.status == Status::Old));
    }

    #[test]
    fn a_missing_lot_or_code_is_told_and_a_feeder_nobody_uses_is_left_out() {
        let r = rows(
            "feeder,lot,date_code\nF1,L1,\nF2,L2,hello\nF4,L4,2301\n",
            "2410",
            104,
        );
        let by = |f: &str| r.iter().find(|x| x.feeder == f).unwrap();
        assert_eq!(by("F1").status, Status::NoCode);
        assert_eq!(by("F2").status, Status::BadCode);
        assert_eq!(by("F3").status, Status::NoLot);
        assert!(r.iter().all(|x| x.feeder != "F4"));
        assert_eq!(problems(&r), 3);
    }

    #[test]
    fn the_text_and_the_table_say_the_same() {
        let r = rows(
            "feeder,lot,date_code\nF1,L1,2301\nF2,L2,2001\n",
            "2410",
            104,
        );
        let said = text(&r, 104);
        assert!(said.contains("feeder F1: lot L1 date code 2301, 61 weeks old: ok"));
        assert!(said.contains("feeder F3: lot - date code -: no lot"));
        assert!(said.ends_with("3 feeders, 2 to look at (the limit is 104 weeks)\n"));
        let csv = csv(&r);
        assert!(csv.starts_with("feeder,lot,date_code,weeks,status\nF1,L1,2301,61,ok\n"));
        assert!(csv.contains("F3,,,,no lot\n"));
    }
}
