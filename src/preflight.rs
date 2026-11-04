//! The checks before a job: everything that can be found out without moving anything. Each finding has a severity, a message and the thing it is about (a part, a feeder or a setting),
//! so that a window can point at it. A job should not be started while there is an error.

use crate::catalog::Catalog;
use crate::config::MachineConfig;
use crate::feeding::Picks;
use crate::parts::{Part, Side};
use crate::planner::AdvancedPlan;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    pub fn name(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

/// What a finding is about.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    Part(String),
    Feeder(String),
    Setting(String),
    Board,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub severity: Severity,
    pub source: Source,
    pub message: String,
}

/// Whether any finding is an error.
pub fn has_errors(checks: &[Check]) -> bool {
    checks.iter().any(|c| c.severity == Severity::Error)
}

/// The findings for the parts of `side`, with the feeders chosen for them and, when there is one, the plan. The list is in a fixed order: the errors first.
pub fn preflight(
    parts: &[Part],
    side: Side,
    picks: &Picks,
    catalog: &Catalog,
    cfg: &MachineConfig,
    plan: Option<&AdvancedPlan>,
) -> Vec<Check> {
    let mut out: Vec<Check> = Vec::new();
    let mut add = |severity: Severity, source: Source, message: String| {
        out.push(Check {
            severity,
            source,
            message,
        })
    };
    let side_parts: Vec<Part> = parts.iter().filter(|p| p.side == side).cloned().collect();
    if side_parts.is_empty() {
        add(
            Severity::Warning,
            Source::Board,
            "there are no parts on this side of the board".to_string(),
        );
    }
    for r in &picks.without_feeder {
        add(
            Severity::Error,
            Source::Part(r.clone()),
            "no feeder holds this part".to_string(),
        );
    }
    for (id, n) in &picks.short {
        add(
            Severity::Error,
            Source::Feeder(id.clone()),
            format!("it is short of {n} parts for this board"),
        );
    }
    for (id, n) in &picks.low {
        add(
            Severity::Warning,
            Source::Feeder(id.clone()),
            format!("only {n} parts will be left after this board"),
        );
    }
    for (kind, n) in catalog.undefined(&side_parts) {
        add(
            Severity::Warning,
            Source::Part(kind),
            format!("{n} parts of this kind are not described in the tables of parts or packages"),
        );
    }
    // the heights: the nozzle must reach into the board and the part must fit under the top of the travel
    let defaults = cfg.part_defaults();
    let mut kinds: BTreeSet<String> = BTreeSet::new();
    for p in &side_parts {
        if !kinds.insert(p.key()) {
            continue;
        }
        let props = catalog.resolve(p, &defaults);
        let label = format!("{} / {}", p.value.trim(), p.package.trim());
        if cfg.board_top_z_mm - props.through_board_mm < cfg.z_min_mm {
            add(
                Severity::Error,
                Source::Part(label.clone()),
                format!(
                    "placing it needs the nozzle at Z {}, below the travel of Z ({})",
                    cfg.board_top_z_mm - props.through_board_mm,
                    cfg.z_min_mm
                ),
            );
        }
        if cfg.board_top_z_mm + props.height_mm > cfg.z_max_mm {
            add(
                Severity::Error,
                Source::Part(label),
                format!(
                    "its top would be at Z {}, above the travel of Z ({})",
                    cfg.board_top_z_mm + props.height_mm,
                    cfg.z_max_mm
                ),
            );
        }
    }
    if let Some(plan) = plan {
        for p in &plan.plan.placements {
            let outside = p.x < cfg.limit_x_min_mm
                || p.x > cfg.limit_x_max_mm
                || p.y < cfg.limit_y_min_mm
                || p.y > cfg.limit_y_max_mm
                || p.pick.0 < cfg.limit_x_min_mm
                || p.pick.0 > cfg.limit_x_max_mm
                || p.pick.1 < cfg.limit_y_min_mm
                || p.pick.1 > cfg.limit_y_max_mm;
            if outside {
                add(
                    Severity::Error,
                    Source::Part(p.reference.clone()),
                    "the place or the pick point is outside the limits of X and Y".to_string(),
                );
            }
        }
        let tips: BTreeSet<&str> = plan.details.iter().map(|d| d.tip.as_str()).collect();
        if tips.len() > 1 && cfg.tip_changer.is_none() {
            add(
                Severity::Warning,
                Source::Setting("tip_changer_x_mm".to_string()),
                "the parts need more than one nozzle tip and the place of the tip changer is not set"
                    .to_string(),
            );
        }
        if plan.tiers > 1 {
            add(
                Severity::Info,
                Source::Setting("rank_step".to_string()),
                format!(
                    "the ranks make {} groups that are placed one after the other",
                    plan.tiers
                ),
            );
        }
    }
    if cfg.fault_limit == 0 {
        add(
            Severity::Info,
            Source::Setting("fault_limit".to_string()),
            "a feeder is never switched off because of its faults".to_string(),
        );
    }
    out.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.source.cmp(&b.source))
            .then_with(|| a.message.cmp(&b.message))
    });
    out
}

/// The findings as text, one per line.
pub fn to_text(checks: &[Check]) -> String {
    let mut out = String::new();
    for c in checks {
        let what = match &c.source {
            Source::Part(p) => format!("part {p}"),
            Source::Feeder(f) => format!("feeder {f}"),
            Source::Setting(s) => format!("setting {s}"),
            Source::Board => "board".to_string(),
        };
        out.push_str(&format!("{}: {what}: {}\n", c.severity.name(), c.message));
    }
    out
}
