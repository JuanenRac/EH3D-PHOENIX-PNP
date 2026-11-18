// EH3D-PHOENIX-PNP - which order of the places gives the fastest job.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The same board planned with every order of the places (`order`) and with every way of changing the nozzle tips (`tip_strategy`), side by side: the batches, the changes of tip, the way
//! the head travels and the seconds the plan takes, with the fastest marked. The choice stays with the person: nothing here changes a configuration. Plain arithmetic and text.

use crate::catalog::Catalog;
use crate::config::{MachineConfig, OrderStrategy, TipStrategy};
use crate::error::PnpError;
use crate::feeding::{FeederSpec, Picks};
use crate::parts::Part;
use crate::planner::{plan_advanced, Options};

/// Every order of the places, in the order they are listed.
pub const ORDERS: [OrderStrategy; 6] = [
    OrderStrategy::Nearest,
    OrderStrategy::Part,
    OrderStrategy::HeightPart,
    OrderStrategy::Pick,
    OrderStrategy::Tips,
    OrderStrategy::None,
];

/// Every way of changing the tips, in the order they are listed.
pub const TIP_STRATEGIES: [TipStrategy; 3] = [
    TipStrategy::Minimize,
    TipStrategy::AsPlanned,
    TipStrategy::Optimize,
];

/// The word for a way of changing the tips in the configuration.
pub fn tip_strategy_name(s: TipStrategy) -> &'static str {
    match s {
        TipStrategy::Minimize => "minimize",
        TipStrategy::AsPlanned => "as_planned",
        TipStrategy::Optimize => "optimize",
    }
}

/// One plan.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The word of the setting that was tried.
    pub name: &'static str,
    pub batches: usize,
    pub tip_changes: usize,
    pub travel_mm: f64,
    pub seconds: f64,
}

/// The two lists of plans.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    /// The board planned with each `order`, the tips as the configuration has them.
    pub orders: Vec<Row>,
    /// The board planned with each `tip_strategy`, the order as the configuration has it.
    pub tips: Vec<Row>,
    /// The `order` and the `tip_strategy` of the configuration.
    pub order_now: &'static str,
    pub tips_now: &'static str,
}

/// The number of the fastest row: the fewest seconds, then the shortest travel, then the first of the list. `None` for an empty list.
pub fn fastest(rows: &[Row]) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (i, r) in rows.iter().enumerate() {
        let better = match best {
            None => true,
            Some(b) => {
                let cur = &rows[b];
                r.seconds < cur.seconds || (r.seconds == cur.seconds && r.travel_mm < cur.travel_mm)
            }
        };
        if better {
            best = Some(i);
        }
    }
    best
}

fn row_of(
    name: &'static str,
    parts: &[Part],
    feeders: &[FeederSpec],
    picks: &Picks,
    catalog: &Catalog,
    cfg: &MachineConfig,
    opts: &Options,
) -> Result<Row, PnpError> {
    let plan = plan_advanced(parts, feeders, picks, catalog, cfg, opts)?;
    Ok(Row {
        name,
        batches: plan.plan.batches,
        tip_changes: plan.tip_changes,
        travel_mm: plan.plan.travel_mm,
        seconds: plan.plan.seconds,
    })
}

/// Plans the board with every order and with every way of changing the tips. A plan that cannot be made (a setting that does not allow it) is the error of the comparison.
pub fn compare(
    parts: &[Part],
    feeders: &[FeederSpec],
    picks: &Picks,
    catalog: &Catalog,
    cfg: &MachineConfig,
    opts: &Options,
) -> Result<Comparison, PnpError> {
    let mut orders = Vec::new();
    for order in ORDERS {
        let mut c = cfg.clone();
        c.order = order;
        orders.push(row_of(
            order.name(),
            parts,
            feeders,
            picks,
            catalog,
            &c,
            opts,
        )?);
    }
    let mut tips = Vec::new();
    for strategy in TIP_STRATEGIES {
        let mut c = cfg.clone();
        c.tip_strategy = strategy;
        tips.push(row_of(
            tip_strategy_name(strategy),
            parts,
            feeders,
            picks,
            catalog,
            &c,
            opts,
        )?);
    }
    Ok(Comparison {
        orders,
        tips,
        order_now: cfg.order.name(),
        tips_now: tip_strategy_name(cfg.tip_strategy),
    })
}

fn table(title: &str, key: &str, rows: &[Row], now: &str) -> String {
    let best = fastest(rows);
    let mut out = format!(
        "{title}\n  {:<12} {:>7} {:>5} {:>10} {:>9}\n",
        key, "batches", "tips", "travel mm", "seconds"
    );
    for (i, r) in rows.iter().enumerate() {
        out.push_str(&format!(
            "  {:<12} {:>7} {:>5} {:>10.1} {:>9.1}{}{}\n",
            r.name,
            r.batches,
            r.tip_changes,
            r.travel_mm,
            r.seconds,
            if r.name == now {
                "  <- the configuration"
            } else {
                ""
            },
            if best == Some(i) { "  <- fastest" } else { "" }
        ));
    }
    out
}

impl Comparison {
    /// The two lists as text, the fastest of each marked and the one the configuration has.
    pub fn text(&self) -> String {
        let mut out = table("order of the places", "order", &self.orders, self.order_now);
        out.push_str(&table(
            "change of the nozzle tips",
            "tip_strategy",
            &self.tips,
            self.tips_now,
        ));
        out
    }

    /// The seconds that the fastest order saves over the one the configuration has (0 when the configuration has the fastest).
    pub fn seconds_saved_by_order(&self) -> f64 {
        let now = self.orders.iter().find(|r| r.name == self.order_now);
        match (fastest(&self.orders), now) {
            (Some(b), Some(n)) => (n.seconds - self.orders[b].seconds).max(0.0),
            _ => 0.0,
        }
    }

    /// The same for the way of changing the tips.
    pub fn seconds_saved_by_tips(&self) -> f64 {
        let now = self.tips.iter().find(|r| r.name == self.tips_now);
        match (fastest(&self.tips), now) {
            (Some(b), Some(n)) => (n.seconds - self.tips[b].seconds).max(0.0),
            _ => 0.0,
        }
    }

    /// A line that says what could be won: the best order and the best way of changing the tips and the seconds each saves, or that the configuration is already the fastest.
    pub fn advice(&self) -> String {
        let mut out = String::new();
        for (what, rows, saved, now) in [
            (
                "order",
                &self.orders,
                self.seconds_saved_by_order(),
                self.order_now,
            ),
            (
                "tip_strategy",
                &self.tips,
                self.seconds_saved_by_tips(),
                self.tips_now,
            ),
        ] {
            let Some(b) = fastest(rows) else { continue };
            if saved > 0.0 {
                out.push_str(&format!(
                    "{what} = {} saves {saved:.1} s over {what} = {now}\n",
                    rows[b].name
                ));
            } else {
                out.push_str(&format!("{what} = {now} is already the fastest\n"));
            }
        }
        out
    }
}
