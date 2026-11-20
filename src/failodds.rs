// EH3D-PHOENIX-PNP - how likely a board is to go through without an error, for the retries that are allowed.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The machine tries again when a feed, a pick or a placement fails, up to the retries of the settings (`feed_retries`, `pick_retries`, `place_retries`), and a part whose tries are all used is an
//! error. How many retries are worth allowing depends on how often things fail, which `simulate-runs` finds by running the job many times. This gives the same answer with arithmetic and no
//! chance: with the chances of failure of the settings (`sim_feed_fail_rate`, `sim_pick_fail_rate`, `sim_place_fail_rate`, `sim_part_fail`, `sim_feeder_fail`) a stage that fails with
//! chance `p` and may be tried `r + 1` times ends in error with chance `p^(r+1)`, and the retries it is expected to use are `p + p^2 + ... + p^r`. A file of rules `pattern,feed_retries,pick_retries,
//! place_retries` gives other retries to the packages that match (the first rule that matches wins; an empty cell keeps the setting), so the retries of a small part and of a large one can
//! be compared. The chances are the ones of the simulated machine: this says what the settings mean, not what a machine does. Nothing is run.

use crate::config::MachineConfig;
use crate::error::{Code, PnpError};
use crate::feeding::{FeederSpec, Picks};
use crate::parts::{csv_field, detect_delimiter, read_records, Part};
use crate::rotation::glob_match;
use std::collections::BTreeMap;

/// The most rows of a file of rules.
pub const MAX_RULES: usize = 1000;
/// The most retries a rule may give.
pub const MAX_RETRIES: u32 = 20;

fn bad(message: impl Into<String>) -> PnpError {
    PnpError::new(Code::ConfigBadValue, message)
}

/// The retries of the packages that match a pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub pattern: String,
    pub feed: Option<u32>,
    pub pick: Option<u32>,
    pub place: Option<u32>,
}

/// Reads the rules: a header with `pattern` and at least one of `feed_retries`, `pick_retries` and `place_retries`, and a row for each rule.
pub fn parse_rules(text: &str) -> Result<Vec<Rule>, PnpError> {
    let text = text.trim_start_matches('\u{feff}');
    let head_line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let records = read_records(text, detect_delimiter(head_line)).map_err(bad)?;
    let Some((header, rows)) = records.split_first() else {
        return Err(bad("the file of rules is empty"));
    };
    let header: Vec<String> = header.iter().map(|h| h.to_ascii_lowercase()).collect();
    let column = |name: &str| header.iter().position(|h| h == name);
    let Some(pattern_at) = column("pattern") else {
        return Err(bad("the file of rules needs the column pattern"));
    };
    let (feed_at, pick_at, place_at) = (
        column("feed_retries"),
        column("pick_retries"),
        column("place_retries"),
    );
    if feed_at.is_none() && pick_at.is_none() && place_at.is_none() {
        return Err(bad(
            "the file of rules needs at least one of feed_retries, pick_retries and place_retries",
        ));
    }
    if rows.len() > MAX_RULES {
        return Err(bad(format!("more than {MAX_RULES} rules")));
    }
    let mut out = Vec::new();
    for (n, row) in rows.iter().enumerate() {
        let line = n + 2;
        let pattern = row.get(pattern_at).cloned().unwrap_or_default();
        if pattern.is_empty() || pattern.chars().count() > 128 {
            return Err(bad(format!(
                "row {line}: the pattern is empty or longer than 128 characters"
            )));
        }
        let count = |at: Option<usize>| -> Result<Option<u32>, PnpError> {
            match at.and_then(|i| row.get(i)).map(String::as_str) {
                None | Some("") => Ok(None),
                Some(v) => v
                    .parse::<u32>()
                    .ok()
                    .filter(|n| *n <= MAX_RETRIES)
                    .map(Some)
                    .ok_or_else(|| bad(format!("row {line}: '{v}' is not a whole number of retries from 0 to {MAX_RETRIES}"))),
            }
        };
        out.push(Rule {
            pattern,
            feed: count(feed_at)?,
            pick: count(pick_at)?,
            place: count(place_at)?,
        });
    }
    Ok(out)
}

/// The chance that a stage ends in error when it is tried `retries + 1` times and fails with chance `p` each time.
pub fn stage_fail(p: f64, retries: u32) -> f64 {
    p.clamp(0.0, 1.0).powi(retries as i32 + 1)
}

/// The retries a stage is expected to use: `p + p^2 + ... + p^retries`.
pub fn stage_retries(p: f64, retries: u32) -> f64 {
    let p = p.clamp(0.0, 1.0);
    (1..=retries).map(|k| p.powi(k as i32)).sum()
}

/// The chance that at least one of independent causes happens.
fn any_of(chances: &[f64]) -> f64 {
    1.0 - chances
        .iter()
        .fold(1.0, |acc, c| acc * (1.0 - c.clamp(0.0, 1.0)))
}

/// For one part of the package `package` picked from the feeder `feeder_id`, with `allowed` feed, pick and placement retries: the chance that it ends in error, the retries it is expected
/// to use and the seconds they take, from the chances of failure of the settings.
pub fn part_odds(
    package: &str,
    feeder_id: &str,
    allowed: (u32, u32, u32),
    cfg: &MachineConfig,
) -> (f64, f64, f64) {
    let (part_pick, part_place) = cfg
        .sim_part_fail
        .iter()
        .find(|(pattern, _, _)| glob_match(pattern, package))
        .map_or((0.0, 0.0), |(_, a, b)| (*a, *b));
    let feeder_pick = cfg
        .sim_feeder_fail
        .iter()
        .find(|(id, _)| id == feeder_id)
        .map_or(0.0, |(_, c)| *c);
    let p_feed = cfg.sim_feed_fail_rate;
    let p_pick = any_of(&[cfg.sim_pick_fail_rate, part_pick, feeder_pick]);
    let p_place = any_of(&[cfg.sim_place_fail_rate, part_place]);
    let fail = any_of(&[
        stage_fail(p_feed, allowed.0),
        stage_fail(p_pick, allowed.1),
        stage_fail(p_place, allowed.2),
    ]);
    let (rf, rp, rl) = (
        stage_retries(p_feed, allowed.0),
        stage_retries(p_pick, allowed.1),
        stage_retries(p_place, allowed.2),
    );
    (
        fail,
        rf + rp + rl,
        rf * cfg.feed_s + rp * cfg.pick_s + rl * cfg.place_s,
    )
}

/// One package of the board.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub package: String,
    pub parts: usize,
    /// The chance that a part of this package ends in error, on the average of its parts.
    pub fail: f64,
    /// The parts of the package that are expected to end in error on a board.
    pub failed: f64,
    /// The retries that a part of the package is expected to use.
    pub retries: f64,
    /// The feed, pick and placement retries that were used for it.
    pub allowed: (u32, u32, u32),
}

/// The answer for a board.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Odds {
    /// The packages, the one that is expected to fail most parts first.
    pub rows: Vec<Row>,
    pub parts: usize,
    /// The parts of a board that are expected to end in error.
    pub failed: f64,
    /// The chance that a board goes through with no part in error.
    pub board_ok: f64,
    /// The retries that a board is expected to use, and the seconds they take.
    pub retries: f64,
    pub retry_s: f64,
    /// The references of the parts that no feeder holds: they are not counted.
    pub without_feeder: Vec<String>,
}

/// The odds for the parts that have a feeder.
pub fn analyse(
    parts: &[Part],
    feeders: &[FeederSpec],
    picks: &Picks,
    cfg: &MachineConfig,
    rules: &[Rule],
) -> Odds {
    let mut by_package: BTreeMap<String, Row> = BTreeMap::new();
    let mut log_ok = 0.0f64;
    let mut zero = false;
    let (mut failed, mut retries, mut retry_s) = (0.0, 0.0, 0.0);
    for pick in &picks.picks {
        let part = &parts[pick.part];
        let feeder = &feeders[pick.feeder];
        let package = part.package.trim();
        let rule = rules.iter().find(|r| glob_match(&r.pattern, package));
        let allowed = (
            rule.and_then(|r| r.feed).unwrap_or(cfg.feed_retries),
            rule.and_then(|r| r.pick).unwrap_or(cfg.pick_retries),
            rule.and_then(|r| r.place).unwrap_or(cfg.place_retries),
        );
        let (fail, used, seconds) = part_odds(package, &feeder.base.id, allowed, cfg);
        if fail >= 1.0 {
            zero = true;
        } else {
            log_ok += (1.0 - fail).ln();
        }
        failed += fail;
        retries += used;
        retry_s += seconds;
        let row = by_package
            .entry(package.to_ascii_lowercase())
            .or_insert_with(|| Row {
                package: package.to_string(),
                parts: 0,
                fail: 0.0,
                failed: 0.0,
                retries: 0.0,
                allowed,
            });
        row.parts += 1;
        row.failed += fail;
        row.retries += used;
    }
    let mut rows: Vec<Row> = by_package.into_values().collect();
    for r in &mut rows {
        r.fail = r.failed / r.parts as f64;
        r.retries /= r.parts as f64;
    }
    rows.sort_by(|a, b| {
        b.failed.total_cmp(&a.failed).then_with(|| {
            a.package
                .to_ascii_lowercase()
                .cmp(&b.package.to_ascii_lowercase())
        })
    });
    let mut without_feeder = picks.without_feeder.clone();
    without_feeder.sort();
    Odds {
        parts: picks.picks.len(),
        rows,
        failed,
        board_ok: if zero { 0.0 } else { log_ok.exp() },
        retries,
        retry_s,
        without_feeder,
    }
}

impl Odds {
    /// The table for a person.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for r in &self.rows {
            out.push_str(&format!(
                "{}: {} parts, retries {}/{}/{} (feed/pick/place), {:.4}% of the parts end in error, {:.3} retries for a part\n",
                r.package,
                r.parts,
                r.allowed.0,
                r.allowed.1,
                r.allowed.2,
                r.fail * 100.0,
                r.retries
            ));
        }
        if !self.without_feeder.is_empty() {
            out.push_str(&format!(
                "no feeder holds: {}\n",
                self.without_feeder.join(" ")
            ));
        }
        out.push_str(&format!(
            "{} parts: {:.4} are expected to end in error on a board, {:.3}% of the boards go through with none, {:.2} retries are expected ({:.2} s)\n",
            self.parts,
            self.failed,
            self.board_ok * 100.0,
            self.retries,
            self.retry_s
        ));
        out
    }

    /// The table as a CSV: `package,parts,feed_retries,pick_retries,place_retries,fail_percent,failed_per_board,retries_per_part`.
    pub fn csv(&self) -> String {
        let mut out = String::from(
            "package,parts,feed_retries,pick_retries,place_retries,fail_percent,failed_per_board,retries_per_part\n",
        );
        for r in &self.rows {
            out.push_str(&format!(
                "{},{},{},{},{},{:.6},{:.6},{:.6}\n",
                csv_field(&r.package),
                r.parts,
                r.allowed.0,
                r.allowed.1,
                r.allowed.2,
                r.fail * 100.0,
                r.failed,
                r.retries
            ));
        }
        out
    }
}
