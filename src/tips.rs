//! The nozzle tips that a package accepts. A rule says that the packages that match a pattern can be picked with any of a list of tips, the preferred one first; a part whose catalog row
//! names a tip keeps that tip (the one tip it accepts). The planner then chooses, for every kind of part, one tip of its list so that the machine needs as few different tips as it can,
//! which is as few changes of tip as it can, and where several choices need as many tips as each other the preferred one wins. With `tip_compat_mode = preferred` every kind just takes
//! the first tip of its list.

use crate::catalog::Catalog;
use crate::config::MachineConfig;
use crate::feeding::Picks;
use crate::parts::{Part, Side};
use crate::rotation::glob_match;
use std::collections::{BTreeMap, HashMap};

/// The most rules a machine can have, and the most tips in one rule.
pub const MAX_TIP_RULES: usize = 64;
pub const MAX_RULE_TIPS: usize = 8;

/// The packages that match `pattern` (`*` and `?`, any case) accept these tips, the preferred one first.
#[derive(Debug, Clone, PartialEq)]
pub struct TipRule {
    pub pattern: String,
    pub tips: Vec<String>,
}

/// How a tip is chosen among those a package accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TipChoice {
    /// The fewest different tips for the whole job; among equal choices the preferred tips.
    #[default]
    FewestChanges,
    /// The first (preferred) tip of the list of each kind.
    Preferred,
}

impl TipChoice {
    pub fn parse(text: &str) -> Option<TipChoice> {
        match text {
            "fewest_changes" => Some(TipChoice::FewestChanges),
            "preferred" => Some(TipChoice::Preferred),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            TipChoice::FewestChanges => "fewest_changes",
            TipChoice::Preferred => "preferred",
        }
    }
}

/// The first rule whose pattern matches the package.
pub fn rule_for<'a>(rules: &'a [TipRule], package: &str) -> Option<&'a TipRule> {
    rules
        .iter()
        .find(|r| glob_match(&r.pattern, package.trim()))
}

/// What one kind of part needs: the tips it accepts (preferred first, never empty) and how many parts of that kind there are.
#[derive(Debug, Clone, PartialEq)]
pub struct Need {
    pub candidates: Vec<String>,
    pub count: usize,
}

/// One tip for every need, in the order of `needs`.
///
/// With `FewestChanges` the tips that a need accepts only are taken first, in the order of the needs; then, while a need is left without a tip it accepts, the tip that covers the most
/// parts of the needs that are left is added (among equals, the one that is better placed in the lists, counting each position by the parts of its need, and then the first by name);
/// finally every need takes the best placed of its tips among those added.
pub fn choose(needs: &[Need], mode: TipChoice) -> Vec<String> {
    let first = |n: &Need| n.candidates.first().cloned().unwrap_or_default();
    if mode == TipChoice::Preferred {
        return needs.iter().map(first).collect();
    }
    let mut chosen: Vec<String> = Vec::new();
    for need in needs.iter().filter(|n| n.candidates.len() == 1) {
        if !chosen.contains(&need.candidates[0]) {
            chosen.push(need.candidates[0].clone());
        }
    }
    loop {
        let uncovered: Vec<&Need> = needs
            .iter()
            .filter(|n| {
                !n.candidates.is_empty() && !n.candidates.iter().any(|t| chosen.contains(t))
            })
            .collect();
        if uncovered.is_empty() {
            break;
        }
        // (parts covered, placing cost) of every tip that an uncovered need accepts
        let mut score: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        for need in &uncovered {
            for (rank, tip) in need.candidates.iter().enumerate() {
                let e = score.entry(tip.as_str()).or_insert((0, 0));
                e.0 += need.count;
                e.1 += rank * need.count;
            }
        }
        let mut best: Option<(&str, (usize, usize))> = None;
        for (tip, s) in score {
            let better = match best {
                None => true,
                Some((_, b)) => s.0 > b.0 || (s.0 == b.0 && s.1 < b.1),
            };
            if better {
                best = Some((tip, s));
            }
        }
        match best {
            Some((tip, _)) => chosen.push(tip.to_string()),
            None => break,
        }
    }
    needs
        .iter()
        .map(|n| {
            n.candidates
                .iter()
                .find(|t| chosen.contains(t))
                .cloned()
                .unwrap_or_else(|| first(n))
        })
        .collect()
}

/// The tip of every kind of part (`Part::key`) that the picks of one side use, when the configuration has rules for tips; empty when it has none, so that the tips are then the ones of
/// the catalog and the defaults, as they always were.
pub fn resolve(
    parts: &[Part],
    picks: &Picks,
    side: Side,
    catalog: &Catalog,
    cfg: &MachineConfig,
) -> HashMap<String, String> {
    if cfg.tip_rules.is_empty() {
        return HashMap::new();
    }
    let defaults = cfg.part_defaults();
    // kind -> (the candidates, the number of parts); a sorted map, so the result does not depend on the order of the files
    let mut kinds: BTreeMap<String, Need> = BTreeMap::new();
    for pick in &picks.picks {
        let Some(p) = parts.get(pick.part).filter(|p| p.side == side) else {
            continue;
        };
        kinds
            .entry(p.key())
            .and_modify(|n| n.count += 1)
            .or_insert_with(|| {
                let candidates = match catalog.explicit_tip(p) {
                    Some(tip) => vec![tip],
                    None => match rule_for(&cfg.tip_rules, &p.package) {
                        Some(rule) => rule.tips.clone(),
                        None => vec![catalog.resolve(p, &defaults).nozzle_tip],
                    },
                };
                Need {
                    candidates,
                    count: 1,
                }
            });
    }
    let needs: Vec<Need> = kinds.values().cloned().collect();
    let tips = choose(&needs, cfg.tip_choice);
    kinds.into_keys().zip(tips).collect()
}
