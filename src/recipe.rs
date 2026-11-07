// EH3D-PHOENIX-PNP - the recipe of a job.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The recipe of a job: a text file, with a version, that fixes the sequence of steps a job goes through (prepare, pick, align, place, check) and the parameters that change for a package, a
//! feeder or a nozzle tip. It is read and checked here without a machine; the job takes the steps from it (the align step and the vacuum check are on only when the recipe has them) and asks it
//! for the parameters of every pick and place.
//!
//! ```text
//! recipe_version = 1
//! name = standard
//! step = prepare
//! step = pick
//! step = align
//! step = place
//! step = check
//! package = SOT-23* : pick_retries=3, pick_tolerance_mm=0.3
//! feeder = F1 : vacuum_suck_s=0.12
//! tip = tip-small : vacuum_blow_s=0.1
//! ```
//!
//! The steps come in this order and none is repeated; `prepare`, `pick` and `place` are always there. A rule is `package`, `feeder` or `tip`, a pattern (`*` and `?`, any case) and one or
//! more `key=value` pairs. For each key the first rule that matches wins, looking at the rules of the feeder first, then those of the package and then those of the tip, each in the order of the
//! file; a key that no rule gives keeps its value in the configuration of the machine.

use crate::config::MachineConfig;
use crate::error::{Code, PnpError};
use crate::rotation::glob_match;

/// The version of the recipe file that this program writes and reads; a file with a higher number is refused.
pub const RECIPE_VERSION: u32 = 1;
/// The most characters of a line of a recipe.
pub const MAX_RECIPE_LINE: usize = 1024;
const MAX_NAME_CHARS: usize = 64;
const MAX_PATTERN_CHARS: usize = 128;

/// A step of a job, in the order in which a job goes through them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Mount or change the nozzle tip of the batch.
    Prepare,
    /// Feed and pick the part.
    Pick,
    /// Look at the part with the camera and correct its place.
    Align,
    /// Put the part on the board.
    Place,
    /// Read the vacuum after the pick and after the place.
    Check,
}

impl Step {
    pub const ALL: [Step; 5] = [
        Step::Prepare,
        Step::Pick,
        Step::Align,
        Step::Place,
        Step::Check,
    ];

    /// The word of the step in the file.
    pub fn name(self) -> &'static str {
        match self {
            Step::Prepare => "prepare",
            Step::Pick => "pick",
            Step::Align => "align",
            Step::Place => "place",
            Step::Check => "check",
        }
    }

    fn parse(text: &str) -> Option<Step> {
        Step::ALL.into_iter().find(|s| s.name() == text)
    }

    fn required(self) -> bool {
        matches!(self, Step::Prepare | Step::Pick | Step::Place)
    }

    fn rank(self) -> usize {
        Step::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

/// What a rule looks at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selector {
    /// The identifier of the feeder the part is picked from.
    Feeder,
    /// The package of the part.
    Package,
    /// The nozzle tip of the part.
    Tip,
}

impl Selector {
    /// The order in which the rules are looked at.
    pub const ALL: [Selector; 3] = [Selector::Feeder, Selector::Package, Selector::Tip];

    pub fn name(self) -> &'static str {
        match self {
            Selector::Feeder => "feeder",
            Selector::Package => "package",
            Selector::Tip => "tip",
        }
    }

    fn parse(text: &str) -> Option<Selector> {
        Selector::ALL.into_iter().find(|s| s.name() == text)
    }
}

/// The parameters that a rule can change; a field that is `None` is not changed.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Params {
    /// How many times a pick is tried again (0 to 10).
    pub pick_retries: Option<u32>,
    /// How far from the nozzle the camera may find the part, in millimetres (0.001 to 50).
    pub pick_tolerance_mm: Option<f64>,
    /// The seconds the pump sucks before the vacuum is read after a pick (0 to 10).
    pub vacuum_suck_s: Option<f64>,
    /// The seconds of blowing before the vacuum is read after a place (0 to 10).
    pub vacuum_blow_s: Option<f64>,
}

impl Params {
    /// True when nothing is changed.
    pub fn is_empty(&self) -> bool {
        *self == Params::default()
    }

    /// These parameters, with those of `other` for what they leave unchanged.
    pub fn or(self, other: Params) -> Params {
        Params {
            pick_retries: self.pick_retries.or(other.pick_retries),
            pick_tolerance_mm: self.pick_tolerance_mm.or(other.pick_tolerance_mm),
            vacuum_suck_s: self.vacuum_suck_s.or(other.vacuum_suck_s),
            vacuum_blow_s: self.vacuum_blow_s.or(other.vacuum_blow_s),
        }
    }
}

/// A rule of a recipe.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub selector: Selector,
    pub pattern: String,
    pub params: Params,
}

/// The limits within which a recipe is read: `recipe_max_bytes` and `recipe_max_rules` of the configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_bytes: u64,
    pub max_rules: usize,
}

impl Limits {
    pub fn from_config(cfg: &MachineConfig) -> Limits {
        Limits {
            max_bytes: cfg.recipe_max_bytes,
            max_rules: cfg.recipe_max_rules,
        }
    }
}

impl Default for Limits {
    fn default() -> Limits {
        Limits::from_config(&MachineConfig::default())
    }
}

/// A recipe that was read and checked.
#[derive(Debug, Clone, PartialEq)]
pub struct Recipe {
    /// The name the file gives it; empty when it gives none.
    pub name: String,
    /// The steps, in the order of the job.
    pub steps: Vec<Step>,
    /// The rules, in the order of the file.
    pub rules: Vec<Rule>,
}

fn bad(line: usize, what: impl std::fmt::Display) -> PnpError {
    PnpError::new(Code::RecipeBad, format!("line {line}: {what}"))
}

fn limit(what: impl std::fmt::Display) -> PnpError {
    PnpError::new(Code::RecipeLimit, what.to_string())
}

fn plain_text(text: &str, max: usize) -> bool {
    !text.is_empty() && text.chars().count() <= max && !text.chars().any(char::is_control)
}

fn number(line: usize, key: &str, value: &str, lo: f64, hi: f64) -> Result<f64, PnpError> {
    let x: f64 = value
        .parse()
        .ok()
        .filter(|x: &f64| x.is_finite())
        .ok_or_else(|| bad(line, format!("{key} = {value} is not a number")))?;
    if x < lo || x > hi {
        return Err(bad(
            line,
            format!("{key} = {value} is outside {lo} to {hi}"),
        ));
    }
    Ok(x)
}

fn parse_params(line: usize, text: &str) -> Result<Params, PnpError> {
    let mut out = Params::default();
    let mut seen: Vec<String> = Vec::new();
    for item in text.split(',') {
        let item = item.trim();
        let Some((k, v)) = item.split_once('=') else {
            return Err(bad(line, format!("'{item}' is not key=value")));
        };
        let (key, value) = (k.trim().to_ascii_lowercase(), v.trim());
        if seen.contains(&key) {
            return Err(bad(line, format!("{key} is written twice in the rule")));
        }
        match key.as_str() {
            "pick_retries" => {
                let n = number(line, &key, value, 0.0, 10.0)?;
                if n.fract() != 0.0 {
                    return Err(bad(line, format!("{key} = {value} is not a whole number")));
                }
                out.pick_retries = Some(n as u32);
            }
            "pick_tolerance_mm" => {
                out.pick_tolerance_mm = Some(number(line, &key, value, 0.001, 50.0)?)
            }
            "vacuum_suck_s" => out.vacuum_suck_s = Some(number(line, &key, value, 0.0, 10.0)?),
            "vacuum_blow_s" => out.vacuum_blow_s = Some(number(line, &key, value, 0.0, 10.0)?),
            _ => return Err(bad(line, format!("{key} is not a parameter of a rule"))),
        }
        seen.push(key);
    }
    Ok(out)
}

fn parse_rule(line: usize, selector: Selector, value: &str) -> Result<Rule, PnpError> {
    let Some((pattern, params)) = value.split_once(':') else {
        return Err(bad(
            line,
            format!("{} needs PATTERN : key=value, ...", selector.name()),
        ));
    };
    let pattern = pattern.trim();
    if !plain_text(pattern, MAX_PATTERN_CHARS) {
        return Err(bad(
            line,
            format!("the pattern is empty, longer than {MAX_PATTERN_CHARS} characters or holds a control character"),
        ));
    }
    Ok(Rule {
        selector,
        pattern: pattern.to_string(),
        params: parse_params(line, params)?,
    })
}

fn check_steps(steps: &[Step]) -> Result<(), PnpError> {
    let words: Vec<&str> = steps.iter().map(|s| s.name()).collect();
    for pair in steps.windows(2) {
        if pair[1].rank() <= pair[0].rank() {
            return Err(PnpError::new(
                Code::RecipeSteps,
                format!(
                    "'{}' cannot come after '{}' (steps: {}; the order is prepare, pick, align, place, check, each at most once)",
                    pair[1].name(),
                    pair[0].name(),
                    words.join(", ")
                ),
            ));
        }
    }
    for s in Step::ALL.into_iter().filter(|s| s.required()) {
        if !steps.contains(&s) {
            return Err(PnpError::new(
                Code::RecipeSteps,
                format!(
                    "the step '{}' is missing (steps: {})",
                    s.name(),
                    if words.is_empty() {
                        "none".to_string()
                    } else {
                        words.join(", ")
                    }
                ),
            ));
        }
    }
    Ok(())
}

impl Recipe {
    /// The recipe with the standard sequence and no rules: what a job does without a recipe when the align step and the vacuum check are on.
    pub fn standard() -> Recipe {
        Recipe {
            name: String::new(),
            steps: Step::ALL.to_vec(),
            rules: Vec::new(),
        }
    }

    /// Reads a recipe. Every error names the line; a file over the limits, a version this program does not know and steps out of order have codes of their own.
    pub fn parse(text: &str, limits: &Limits) -> Result<Recipe, PnpError> {
        if text.len() as u64 > limits.max_bytes {
            return Err(limit(format!(
                "the recipe has {} bytes, the limit is {}",
                text.len(),
                limits.max_bytes
            )));
        }
        let mut version: Option<u32> = None;
        let mut name: Option<String> = None;
        let mut steps: Vec<Step> = Vec::new();
        let mut rules: Vec<Rule> = Vec::new();
        for (i, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
            let line = i + 1;
            if raw.chars().count() > MAX_RECIPE_LINE {
                return Err(limit(format!(
                    "line {line} has more than {MAX_RECIPE_LINE} characters"
                )));
            }
            let raw = raw.trim();
            if raw.is_empty() || raw.starts_with('#') {
                continue;
            }
            let Some((k, v)) = raw.split_once('=') else {
                return Err(bad(line, "it is not key = value"));
            };
            let (key, value) = (k.trim().to_ascii_lowercase(), v.trim());
            if key == "recipe_version" {
                if version.is_some() {
                    return Err(bad(line, "recipe_version is written twice"));
                }
                let n: u32 = value
                    .parse()
                    .ok()
                    .filter(|n| *n >= 1)
                    .ok_or_else(|| bad(line, "recipe_version is not a whole number from 1"))?;
                if n > RECIPE_VERSION {
                    return Err(PnpError::new(
                        Code::RecipeNewer,
                        format!("line {line}: recipe_version {n}, this program reads up to {RECIPE_VERSION}"),
                    ));
                }
                version = Some(n);
                continue;
            }
            if version.is_none() {
                return Err(bad(line, "recipe_version must be the first key"));
            }
            match key.as_str() {
                "name" => {
                    if name.is_some() {
                        return Err(bad(line, "name is written twice"));
                    }
                    if !plain_text(value, MAX_NAME_CHARS) {
                        return Err(bad(
                            line,
                            format!("the name is empty, longer than {MAX_NAME_CHARS} characters or holds a control character"),
                        ));
                    }
                    name = Some(value.to_string());
                }
                "step" => match Step::parse(&value.to_ascii_lowercase()) {
                    Some(s) => steps.push(s),
                    None => {
                        return Err(bad(
                            line,
                            format!(
                                "'{value}' is not a step (prepare, pick, align, place or check)"
                            ),
                        ))
                    }
                },
                other => {
                    let Some(selector) = Selector::parse(other) else {
                        return Err(bad(line, format!("{other} is not a key of a recipe")));
                    };
                    let rule = parse_rule(line, selector, value)?;
                    if rules.iter().any(|r| {
                        r.selector == selector && r.pattern.eq_ignore_ascii_case(&rule.pattern)
                    }) {
                        return Err(bad(
                            line,
                            format!("{} '{}' has a rule already", selector.name(), rule.pattern),
                        ));
                    }
                    if rules.len() >= limits.max_rules {
                        return Err(limit(format!(
                            "line {line}: more than {} rules",
                            limits.max_rules
                        )));
                    }
                    rules.push(rule);
                }
            }
        }
        if version.is_none() {
            return Err(bad(1, "there is no recipe_version"));
        }
        check_steps(&steps)?;
        Ok(Recipe {
            name: name.unwrap_or_default(),
            steps,
            rules,
        })
    }

    /// Whether the recipe has the step.
    pub fn has(&self, step: Step) -> bool {
        self.steps.contains(&step)
    }

    /// The configuration of the machine with the sequence of the recipe: the align step is on when the recipe has it, and the vacuum check when it has `check`.
    pub fn apply(&self, cfg: &MachineConfig) -> MachineConfig {
        let mut out = cfg.clone();
        out.align = self.has(Step::Align);
        out.vacuum = self.has(Step::Check);
        out
    }

    /// The parameters for a part that comes from `feeder`, of the package `package`, with the nozzle tip `tip`: for each key the first matching rule of the feeders, then of the packages,
    /// then of the tips.
    pub fn resolve(&self, feeder: &str, package: &str, tip: &str) -> Params {
        let mut out = Params::default();
        for selector in Selector::ALL {
            let text = match selector {
                Selector::Feeder => feeder,
                Selector::Package => package,
                Selector::Tip => tip,
            }
            .trim();
            for r in self
                .rules
                .iter()
                .filter(|r| r.selector == selector && glob_match(&r.pattern, text))
            {
                out = out.or(r.params);
            }
        }
        out
    }

    /// True when the recipe changes the parameters of some parts.
    pub fn has_rules(&self) -> bool {
        self.rules.iter().any(|r| !r.params.is_empty())
    }

    /// The text that stands for the rules in the identity of a saved job, so that a state saved with other rules is not resumed by mistake.
    pub fn fingerprint(&self) -> String {
        let mut out = String::new();
        for r in &self.rules {
            let p = &r.params;
            out.push_str(&format!(
                "{}:{}:{:?}:{:?}:{:?}:{:?};",
                r.selector.name(),
                r.pattern.to_ascii_lowercase(),
                p.pick_retries,
                p.pick_tolerance_mm,
                p.vacuum_suck_s,
                p.vacuum_blow_s
            ));
        }
        out
    }

    /// The sentence that `recipe-check` prints for a recipe that is valid.
    pub fn summary(&self) -> String {
        let words: Vec<&str> = self.steps.iter().map(|s| s.name()).collect();
        format!(
            "ok: recipe '{}', {} steps ({}), {} rules",
            self.name,
            self.steps.len(),
            words.join(", "),
            self.rules.len()
        )
    }
}
