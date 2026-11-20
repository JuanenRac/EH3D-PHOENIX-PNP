// EH3D-PHOENIX-PNP - how many times each part may be tried again, by rules that look at the part.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! `failure-odds` lets the retries differ by package. A board has parts that deserve more: the one expensive chip that must not be given up on after two tries, the reference that sits by a
//! tall connector, the reel of an old tape that mis-feeds. A table of rules `field,pattern[,feed_retries][,pick_retries][,place_retries]` says it: `field` is `reference`, `value`, `package`
//! or `feeder`, `pattern` is a name with `*` and `?`, and the first rule that fits a part (in the order of the file) gives its retries; what a rule leaves empty, and the parts no rule fits,
//! take the retries of the settings. This works out for each part which rule it got and how that changes its chance of ending in error, and says which rules fit nothing (a typing mistake
//! in a pattern) and which fit only parts that an earlier rule took (shadowed, so they never act).

use crate::config::MachineConfig;
use crate::error::{Code, PnpError};
use crate::failodds::{part_odds, MAX_RETRIES};
use crate::feeding::{FeederSpec, Picks};
use crate::parts::{csv_field, detect_delimiter, read_records, Part};
use crate::rotation::glob_match;

/// The most rules.
pub const MAX_RULES: usize = 500;

fn bad(detail: impl Into<String>) -> PnpError {
    PnpError::new(Code::ConfigBadValue, detail.into())
}

/// What a rule looks at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Reference,
    Value,
    Package,
    Feeder,
}

impl Field {
    pub fn name(self) -> &'static str {
        match self {
            Field::Reference => "reference",
            Field::Value => "value",
            Field::Package => "package",
            Field::Feeder => "feeder",
        }
    }

    fn parse(text: &str) -> Option<Field> {
        match text.trim().to_ascii_lowercase().as_str() {
            "reference" | "ref" => Some(Field::Reference),
            "value" => Some(Field::Value),
            "package" => Some(Field::Package),
            "feeder" => Some(Field::Feeder),
            _ => None,
        }
    }
}

/// One rule.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub field: Field,
    pub pattern: String,
    pub feed: Option<u32>,
    pub pick: Option<u32>,
    pub place: Option<u32>,
}

/// Reads the rules.
pub fn parse_rules(text: &str) -> Result<Vec<Rule>, PnpError> {
    let text = text.trim_start_matches('\u{feff}');
    let head_line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let records = read_records(text, detect_delimiter(head_line)).map_err(bad)?;
    let Some((header, rows)) = records.split_first() else {
        return Err(bad("the file of rules is empty"));
    };
    let header: Vec<String> = header
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();
    let column = |name: &str| header.iter().position(|h| h == name);
    let (Some(field_at), Some(pattern_at)) = (column("field"), column("pattern")) else {
        return Err(bad("the file of rules needs the columns field and pattern"));
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
    let mut out = Vec::with_capacity(rows.len());
    for (n, row) in rows.iter().enumerate() {
        let line = n + 2;
        let get = |at: usize| row.get(at).map(|s| s.trim()).unwrap_or("");
        let field = Field::parse(get(field_at)).ok_or_else(|| {
            bad(format!(
                "row {line}: the field is reference, value, package or feeder"
            ))
        })?;
        let pattern = get(pattern_at).to_string();
        if pattern.is_empty()
            || pattern.chars().count() > 128
            || pattern.chars().any(char::is_control)
        {
            return Err(bad(format!(
                "row {line}: the pattern is empty, has a control character or is longer than 128 characters"
            )));
        }
        let count = |at: Option<usize>| -> Result<Option<u32>, PnpError> {
            match at.map(get) {
                None | Some("") => Ok(None),
                Some(v) => v
                    .parse::<u32>()
                    .ok()
                    .filter(|n| *n <= MAX_RETRIES)
                    .map(Some)
                    .ok_or_else(|| {
                        bad(format!(
                            "row {line}: '{v}' is not a whole number of retries from 0 to {MAX_RETRIES}"
                        ))
                    }),
            }
        };
        let rule = Rule {
            field,
            pattern,
            feed: count(feed_at)?,
            pick: count(pick_at)?,
            place: count(place_at)?,
        };
        if rule.feed.is_none() && rule.pick.is_none() && rule.place.is_none() {
            return Err(bad(format!("row {line}: the rule gives no retries")));
        }
        out.push(rule);
    }
    Ok(out)
}

fn fits(rule: &Rule, part: &Part, feeder: &str) -> bool {
    let text = match rule.field {
        Field::Reference => part.reference.trim(),
        Field::Value => part.value.trim(),
        Field::Package => part.package.trim(),
        Field::Feeder => feeder,
    };
    glob_match(&rule.pattern, text)
}

/// A part and the retries it got.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub reference: String,
    pub feeder: String,
    /// The number of the rule (from 1) that gave the retries, or `None` for the settings.
    pub rule: Option<usize>,
    pub allowed: (u32, u32, u32),
    /// The chance that the part ends in error, the retries it is expected to use and the seconds they take.
    pub fail: f64,
    pub retries: f64,
    pub retry_s: f64,
}

/// What the rules do to a board.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Policy {
    /// A row for each part that has a feeder, in the order of the references.
    pub rows: Vec<Row>,
    /// For each rule (in order): the parts it gave retries to, and the parts it fits that an earlier rule took.
    pub rules: Vec<(usize, usize)>,
    /// The chance that a board goes through with no part in error.
    pub board_ok: f64,
    pub retry_s: f64,
    /// The references of the parts that no feeder holds.
    pub without_feeder: Vec<String>,
}

/// Gives each part the retries of the first rule that fits it.
pub fn apply(
    parts: &[Part],
    feeders: &[FeederSpec],
    picks: &Picks,
    cfg: &MachineConfig,
    rules: &[Rule],
) -> Policy {
    let mut policy = Policy {
        rules: vec![(0, 0); rules.len()],
        ..Policy::default()
    };
    let (mut log_ok, mut zero) = (0.0f64, false);
    for pick in &picks.picks {
        let part = &parts[pick.part];
        let feeder = feeders[pick.feeder].base.id.as_str();
        let mut chosen: Option<usize> = None;
        for (n, rule) in rules.iter().enumerate() {
            if fits(rule, part, feeder) {
                if chosen.is_none() {
                    chosen = Some(n);
                    policy.rules[n].0 += 1;
                } else {
                    policy.rules[n].1 += 1;
                }
            }
        }
        let rule = chosen.map(|n| &rules[n]);
        let allowed = (
            rule.and_then(|r| r.feed).unwrap_or(cfg.feed_retries),
            rule.and_then(|r| r.pick).unwrap_or(cfg.pick_retries),
            rule.and_then(|r| r.place).unwrap_or(cfg.place_retries),
        );
        let (fail, retries, retry_s) = part_odds(part.package.trim(), feeder, allowed, cfg);
        if fail >= 1.0 {
            zero = true;
        } else {
            log_ok += (1.0 - fail).ln();
        }
        policy.retry_s += retry_s;
        policy.rows.push(Row {
            reference: part.reference.clone(),
            feeder: feeder.to_string(),
            rule: chosen.map(|n| n + 1),
            allowed,
            fail,
            retries,
            retry_s,
        });
    }
    policy.rows.sort_by(|a, b| a.reference.cmp(&b.reference));
    policy.board_ok = if zero { 0.0 } else { log_ok.exp() };
    policy.without_feeder = picks.without_feeder.clone();
    policy.without_feeder.sort();
    policy
}

impl Policy {
    /// The rules that fit no part at all.
    pub fn unused(&self) -> Vec<usize> {
        self.rules
            .iter()
            .enumerate()
            .filter(|(_, (acted, shadowed))| *acted == 0 && *shadowed == 0)
            .map(|(n, _)| n + 1)
            .collect()
    }

    /// The rules that fit parts but never act, because an earlier rule took every one of them.
    pub fn shadowed(&self) -> Vec<usize> {
        self.rules
            .iter()
            .enumerate()
            .filter(|(_, (acted, shadowed))| *acted == 0 && *shadowed > 0)
            .map(|(n, _)| n + 1)
            .collect()
    }

    /// The report for a person.
    pub fn text(&self, rules: &[Rule]) -> String {
        let mut out = String::new();
        for (n, r) in rules.iter().enumerate() {
            let (acted, shadowed) = self.rules[n];
            out.push_str(&format!(
                "rule {} ({} {}): {} parts{}\n",
                n + 1,
                r.field.name(),
                r.pattern,
                acted,
                if shadowed > 0 {
                    format!(", {shadowed} more fit but an earlier rule took them")
                } else {
                    String::new()
                }
            ));
        }
        let by_settings = self.rows.iter().filter(|r| r.rule.is_none()).count();
        out.push_str(&format!("the settings: {by_settings} parts\n"));
        let unused = self.unused();
        if !unused.is_empty() {
            out.push_str(&format!("rules that fit no part: {}\n", join(&unused)));
        }
        let shadowed = self.shadowed();
        if !shadowed.is_empty() {
            out.push_str(&format!("rules that never act: {}\n", join(&shadowed)));
        }
        if !self.without_feeder.is_empty() {
            out.push_str(&format!(
                "no feeder holds: {}\n",
                self.without_feeder.join(" ")
            ));
        }
        out.push_str(&format!(
            "{:.3}% of the boards go through with no part in error, {:.2} s of retries are expected\n",
            self.board_ok * 100.0,
            self.retry_s
        ));
        out
    }

    /// A row for each part: `reference,feeder,rule,feed_retries,pick_retries,place_retries,fail_percent,retries,retry_s`.
    pub fn csv(&self) -> String {
        let mut out = String::from(
            "reference,feeder,rule,feed_retries,pick_retries,place_retries,fail_percent,retries,retry_s\n",
        );
        for r in &self.rows {
            out.push_str(&format!(
                "{},{},{},{},{},{},{:.6},{:.6},{:.4}\n",
                csv_field(&r.reference),
                csv_field(&r.feeder),
                r.rule.map_or(String::new(), |n| n.to_string()),
                r.allowed.0,
                r.allowed.1,
                r.allowed.2,
                r.fail * 100.0,
                r.retries,
                r.retry_s
            ));
        }
        out
    }
}

fn join(numbers: &[usize]) -> String {
    numbers
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(" ")
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

    fn setup() -> (Vec<Part>, Vec<FeederSpec>, Picks, MachineConfig) {
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
        let cfg = MachineConfig::parse(
            "sim_pick_fail_rate = 0.1\nsim_place_fail_rate = 0.02\nfeed_retries = 1\npick_retries = 2\nplace_retries = 1\n",
        )
        .unwrap();
        (parts, feeders, picks, cfg)
    }

    fn run(rules: &str) -> Policy {
        let (p, f, k, c) = setup();
        apply(&p, &f, &k, &c, &parse_rules(rules).unwrap())
    }

    #[test]
    fn the_rules_are_read() {
        let r = parse_rules(
            "field,pattern,feed_retries,pick_retries,place_retries\nreference,U*,,5,\nPackage,0?03,1,,2\n",
        )
        .unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].field, Field::Reference);
        assert_eq!((r[0].feed, r[0].pick, r[0].place), (None, Some(5), None));
        assert_eq!(r[1].field, Field::Package);
        assert_eq!((r[1].feed, r[1].pick, r[1].place), (Some(1), None, Some(2)));
    }

    #[test]
    fn rules_that_are_wrong_are_refused() {
        for text in [
            "",
            "field,pattern\npackage,x\n",
            "pattern,pick_retries\nx,1\n",
            "field,pattern,pick_retries\ncolour,x,1\n",
            "field,pattern,pick_retries\npackage,,1\n",
            "field,pattern,pick_retries\npackage,x,21\n",
            "field,pattern,pick_retries\npackage,x,-1\n",
            "field,pattern,pick_retries\npackage,x,one\n",
            "field,pattern,pick_retries\npackage,x,\n",
        ] {
            assert!(parse_rules(text).is_err(), "{text:?}");
        }
    }

    #[test]
    fn the_first_rule_that_fits_gives_the_retries() {
        let p = run("field,pattern,pick_retries\nreference,U1,9\npackage,TQFP*,4\nvalue,10k,3\n");
        let by = |r: &str| p.rows.iter().find(|x| x.reference == r).unwrap().clone();
        assert_eq!((by("U1").rule, by("U1").allowed), (Some(1), (1, 9, 1)));
        assert_eq!((by("R1").rule, by("R1").allowed), (Some(3), (1, 3, 1)));
        assert_eq!((by("C1").rule, by("C1").allowed), (None, (1, 2, 1)));
        // the second rule fits U1 too, but the first took it
        assert_eq!(p.rules, vec![(1, 0), (0, 1), (2, 0)]);
        assert_eq!(p.shadowed(), vec![2]);
        assert!(p.unused().is_empty());
    }

    #[test]
    fn more_retries_make_a_part_less_likely_to_end_in_error() {
        let few = run("field,pattern,pick_retries\nreference,U1,0\n");
        let many = run("field,pattern,pick_retries\nreference,U1,6\n");
        let u = |p: &Policy| p.rows.iter().find(|r| r.reference == "U1").unwrap().clone();
        assert!(u(&few).fail > u(&many).fail);
        assert!(u(&few).retries < u(&many).retries);
        assert!(few.board_ok < many.board_ok);
        // the other parts do not change
        let c = |p: &Policy| p.rows.iter().find(|r| r.reference == "C1").unwrap().clone();
        assert_eq!(c(&few), c(&many));
    }

    #[test]
    fn a_rule_that_fits_nothing_is_told() {
        let p = run("field,pattern,pick_retries\nreference,Q*,3\nfeeder,F2,1\n");
        assert_eq!(p.unused(), vec![1]);
        let c1 = p.rows.iter().find(|r| r.reference == "C1").unwrap();
        assert_eq!((c1.rule, c1.allowed.1), (Some(2), 1));
        let said = p.text(
            &parse_rules("field,pattern,pick_retries\nreference,Q*,3\nfeeder,F2,1\n").unwrap(),
        );
        assert!(said.contains("rule 1 (reference Q*): 0 parts"));
        assert!(said.contains("rules that fit no part: 1"));
    }

    #[test]
    fn with_no_rule_the_settings_give_every_part_their_retries() {
        let (parts, feeders, picks, cfg) = setup();
        let none = apply(&parts, &feeders, &picks, &cfg, &[]);
        assert!(none
            .rows
            .iter()
            .all(|r| r.rule.is_none() && r.allowed == (1, 2, 1)));
        let like_odds = crate::failodds::analyse(&parts, &feeders, &picks, &cfg, &[]);
        assert!((none.board_ok - like_odds.board_ok).abs() < 1e-12);
        assert!((none.retry_s - like_odds.retry_s).abs() < 1e-9);
    }

    #[test]
    fn the_table_has_a_row_for_each_part_in_the_order_of_the_references() {
        let p = run("field,pattern,pick_retries\nreference,U1,9\n");
        let csv = p.csv();
        let refs: Vec<&str> = csv
            .lines()
            .skip(1)
            .map(|l| l.split(',').next().unwrap())
            .collect();
        assert_eq!(refs, ["C1", "R1", "R2", "U1"]);
        assert!(csv.lines().any(|l| l.starts_with("U1,F3,1,1,9,1,")));
        assert!(csv.lines().any(|l| l.starts_with("C1,F2,,1,2,1,")));
    }
}
