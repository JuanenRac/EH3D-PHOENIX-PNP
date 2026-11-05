// EH3D-PHOENIX-PNP - the tables of the window.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The rows of the tables of the window (the plan, the parts, the feeders, the kinds of part), how they are sorted, and the selection that the tables and the view share: a part chosen
//! in one place is the part chosen in all of them, and with it the feeder it comes from. A cell is edited with a second click on a row that is already chosen.

use super::workspace::{Planned, Workspace};
use crate::catalog::{PartProps, Resolved};
use crate::feeding::FeederKind;
use crate::job::{Job, Status};
use crate::parts::Side;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

/// Compares two names so that `R2` comes before `R10`: the digits are compared as numbers.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let mut n = String::new();
                while let Some(&c) = x.peek() {
                    if !c.is_ascii_digit() {
                        break;
                    }
                    n.push(c);
                    x.next();
                }
                let mut m = String::new();
                while let Some(&d) = y.peek() {
                    if !d.is_ascii_digit() {
                        break;
                    }
                    m.push(d);
                    y.next();
                }
                let (n, m) = (n.trim_start_matches('0'), m.trim_start_matches('0'));
                let order = n.len().cmp(&m.len()).then_with(|| n.cmp(m));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(c), Some(d)) => {
                let order = c.to_ascii_lowercase().cmp(&d.to_ascii_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
                x.next();
                y.next();
            }
        }
    }
}

/// The column a table is sorted by and the direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortState<C> {
    pub column: C,
    pub ascending: bool,
}

impl<C: Copy + PartialEq> SortState<C> {
    pub fn new(column: C) -> SortState<C> {
        SortState {
            column,
            ascending: true,
        }
    }

    /// A click on the head of a column: the same column turns the direction round, another column starts ascending.
    pub fn click(&mut self, column: C) {
        if self.column == column {
            self.ascending = !self.ascending;
        } else {
            self.column = column;
            self.ascending = true;
        }
    }

    /// An arrow for the head of a column that is the sorted one.
    pub fn mark(&self, column: C) -> &'static str {
        if self.column != column {
            ""
        } else if self.ascending {
            " ^"
        } else {
            " v"
        }
    }
}

fn directed(order: Ordering, ascending: bool) -> Ordering {
    if ascending {
        order
    } else {
        order.reverse()
    }
}

/// What is chosen: a part, and the feeder it comes from (or a feeder chosen on its own).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    pub reference: Option<String>,
    pub feeder: Option<String>,
}

impl Selection {
    /// Chooses a part (and the feeder the plan takes it from, when there is a plan).
    pub fn select_part(&mut self, reference: &str, planned: Option<&Planned>) {
        self.feeder = planned.and_then(|p| {
            p.plan
                .plan
                .placements
                .iter()
                .find(|x| x.reference == reference)
                .map(|x| x.feeder.clone())
        });
        self.reference = Some(reference.to_string());
    }

    /// Chooses a feeder; the part is no longer chosen.
    pub fn select_feeder(&mut self, id: &str) {
        self.feeder = Some(id.to_string());
        self.reference = None;
    }

    pub fn clear(&mut self) {
        *self = Selection::default();
    }

    pub fn is_part(&self, reference: &str) -> bool {
        self.reference.as_deref() == Some(reference)
    }

    pub fn is_feeder(&self, id: &str) -> bool {
        self.feeder.as_deref() == Some(id)
    }
}

/// The cell that is being edited, with the text typed so far.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CellEdit {
    pub feeder: Option<String>,
    pub text: String,
}

impl CellEdit {
    /// A click on the count of a feeder: the first click chooses the row, the second starts the edit. Gives true when the edit started.
    pub fn click_count(&mut self, sel: &mut Selection, id: &str, current: u32) -> bool {
        if sel.is_feeder(id) {
            self.feeder = Some(id.to_string());
            self.text = current.to_string();
            true
        } else {
            sel.select_feeder(id);
            self.feeder = None;
            false
        }
    }

    /// Ends the edit. Gives the feeder and the new count when the text is a whole number; None when it is not (the edit is dropped either way).
    pub fn finish(&mut self) -> Option<(String, u32)> {
        let id = self.feeder.take()?;
        let n = self.text.trim().parse::<u32>().ok()?;
        self.text.clear();
        Some((id, n))
    }

    pub fn cancel(&mut self) {
        self.feeder = None;
        self.text.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartColumn {
    Reference,
    Value,
    Package,
    X,
    Y,
    Rotation,
    Side,
    Feeder,
    Status,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartRow {
    pub reference: String,
    pub value: String,
    pub package: String,
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub side: Side,
    /// The feeder the plan takes it from (empty when there is no plan or no feeder holds it).
    pub feeder: String,
    /// What the job did with it, when a job is running.
    pub status: Option<Status>,
    /// Left out of the plan by a fix.
    pub excluded: bool,
}

fn status_order(s: Option<Status>) -> u8 {
    match s {
        None => 0,
        Some(Status::Pending) => 1,
        Some(Status::Deferred) => 2,
        Some(Status::Placed) => 3,
    }
}

/// The rows of the table of parts.
pub fn part_rows(ws: &Workspace, planned: Option<&Planned>, job: Option<&Job>) -> Vec<PartRow> {
    let mut by_reference: HashMap<&str, (&str, Option<Status>)> = HashMap::new();
    if let Some(p) = planned {
        for (i, x) in p.plan.plan.placements.iter().enumerate() {
            let status = job.and_then(|j| j.status().get(i).copied());
            by_reference.insert(x.reference.as_str(), (x.feeder.as_str(), status));
        }
    }
    ws.parts
        .iter()
        .map(|p| {
            let (feeder, status) = by_reference
                .get(p.reference.as_str())
                .copied()
                .unwrap_or(("", None));
            PartRow {
                reference: p.reference.clone(),
                value: p.value.clone(),
                package: p.package.clone(),
                x: p.x,
                y: p.y,
                rotation: p.rotation,
                side: p.side,
                feeder: feeder.to_string(),
                status,
                excluded: ws.excluded.contains(&p.reference),
            }
        })
        .collect()
}

pub fn sort_parts(rows: &mut [PartRow], by: SortState<PartColumn>) {
    rows.sort_by(|a, b| {
        let order = match by.column {
            PartColumn::Reference => natural_cmp(&a.reference, &b.reference),
            PartColumn::Value => natural_cmp(&a.value, &b.value),
            PartColumn::Package => natural_cmp(&a.package, &b.package),
            PartColumn::X => a.x.total_cmp(&b.x),
            PartColumn::Y => a.y.total_cmp(&b.y),
            PartColumn::Rotation => a.rotation.total_cmp(&b.rotation),
            PartColumn::Side => (a.side == Side::Bottom).cmp(&(b.side == Side::Bottom)),
            PartColumn::Feeder => natural_cmp(&a.feeder, &b.feeder),
            PartColumn::Status => status_order(a.status).cmp(&status_order(b.status)),
        };
        directed(order, by.ascending).then_with(|| natural_cmp(&a.reference, &b.reference))
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanColumn {
    Step,
    Reference,
    Value,
    Feeder,
    Nozzle,
    Batch,
    Tip,
    Status,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanRow {
    /// The place in the plan, from 1.
    pub step: usize,
    pub reference: String,
    pub value: String,
    pub feeder: String,
    /// The nozzle, from 1.
    pub nozzle: usize,
    /// The batch, from 1.
    pub batch: usize,
    pub tip: String,
    pub status: Option<Status>,
    /// True on the first row of a batch whose nozzle tip differs from the one before: the tip is changed there.
    pub tip_change: bool,
}

/// The rows of the plan in the order the head places the parts.
pub fn plan_rows(planned: &Planned, job: Option<&Job>) -> Vec<PlanRow> {
    let values: HashMap<&str, &str> = planned
        .parts
        .iter()
        .map(|p| (p.reference.as_str(), p.value.as_str()))
        .collect();
    let mut out = Vec::with_capacity(planned.plan.plan.placements.len());
    let mut last_tip: Option<&str> = None;
    let mut last_batch: Option<usize> = None;
    for (i, p) in planned.plan.plan.placements.iter().enumerate() {
        let tip = planned.plan.details.get(i).map_or("", |d| d.tip.as_str());
        let new_batch = last_batch != Some(p.batch);
        let tip_change = new_batch && last_tip.is_some_and(|t| t != tip);
        if new_batch {
            last_tip = Some(tip);
        }
        last_batch = Some(p.batch);
        out.push(PlanRow {
            step: i + 1,
            reference: p.reference.clone(),
            value: values
                .get(p.reference.as_str())
                .copied()
                .unwrap_or("")
                .to_string(),
            feeder: p.feeder.clone(),
            nozzle: p.nozzle + 1,
            batch: p.batch + 1,
            tip: tip.to_string(),
            status: job.and_then(|j| j.status().get(i).copied()),
            tip_change,
        });
    }
    out
}

pub fn sort_plan(rows: &mut [PlanRow], by: SortState<PlanColumn>) {
    rows.sort_by(|a, b| {
        let order = match by.column {
            PlanColumn::Step => a.step.cmp(&b.step),
            PlanColumn::Reference => natural_cmp(&a.reference, &b.reference),
            PlanColumn::Value => natural_cmp(&a.value, &b.value),
            PlanColumn::Feeder => natural_cmp(&a.feeder, &b.feeder),
            PlanColumn::Nozzle => a.nozzle.cmp(&b.nozzle),
            PlanColumn::Batch => a.batch.cmp(&b.batch),
            PlanColumn::Tip => natural_cmp(&a.tip, &b.tip),
            PlanColumn::Status => status_order(a.status).cmp(&status_order(b.status)),
        };
        directed(order, by.ascending).then_with(|| a.step.cmp(&b.step))
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeederColumn {
    Id,
    Value,
    Package,
    Count,
    Left,
    Slot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FeederRow {
    pub id: String,
    pub value: String,
    pub package: String,
    pub x: f64,
    pub y: f64,
    pub count: u32,
    /// What is left of the tape: the count minus what the job took.
    pub left: u32,
    pub slot: String,
    pub enabled: bool,
    /// `tape`, `tray` or `strip`.
    pub kind: &'static str,
    /// -1 low, 0 normal, 1 high.
    pub priority: i32,
}

/// The rows of the table of feeders; with a job, how many parts are left in each.
pub fn feeder_rows(ws: &Workspace, job: Option<&Job>) -> Vec<FeederRow> {
    let picked: BTreeMap<&str, u32> = job
        .map(|j| {
            j.stats()
                .feeders
                .iter()
                .map(|f| (f.id.as_str(), f.picked))
                .collect()
        })
        .unwrap_or_default();
    ws.feeders
        .iter()
        .map(|f| {
            let taken = picked.get(f.base.id.as_str()).copied().unwrap_or(0);
            FeederRow {
                id: f.base.id.clone(),
                value: f.base.value.clone(),
                package: f.base.package.clone(),
                x: f.base.x,
                y: f.base.y,
                count: f.base.count,
                left: f.base.count.saturating_sub(taken),
                slot: f.slot.clone().unwrap_or_default(),
                enabled: f.enabled,
                kind: match f.kind {
                    FeederKind::Tape => "tape",
                    FeederKind::Tray { .. } => "tray",
                    FeederKind::Strip { .. } => "strip",
                },
                priority: f.priority,
            }
        })
        .collect()
}

pub fn sort_feeders(rows: &mut [FeederRow], by: SortState<FeederColumn>) {
    rows.sort_by(|a, b| {
        let order = match by.column {
            FeederColumn::Id => natural_cmp(&a.id, &b.id),
            FeederColumn::Value => natural_cmp(&a.value, &b.value),
            FeederColumn::Package => natural_cmp(&a.package, &b.package),
            FeederColumn::Count => a.count.cmp(&b.count),
            FeederColumn::Left => a.left.cmp(&b.left),
            FeederColumn::Slot => natural_cmp(&a.slot, &b.slot),
        };
        directed(order, by.ascending).then_with(|| natural_cmp(&a.id, &b.id))
    });
}

/// A kind of part (a value in a package) with what the tables say about it.
#[derive(Debug, Clone, PartialEq)]
pub struct KindRow {
    pub value: String,
    pub package: String,
    pub count: usize,
    pub height_mm: f64,
    pub rank: i32,
    pub tip: String,
    pub tape_mm: f64,
    /// Whether a table of packages or of parts describes the kind.
    pub described: bool,
}

/// The kinds of part of the side being planned, most numerous first.
pub fn kind_rows(ws: &Workspace) -> Vec<KindRow> {
    let defaults = ws.cfg.part_defaults();
    let mut kinds: BTreeMap<String, KindRow> = BTreeMap::new();
    for p in ws.parts.iter().filter(|p| p.side == ws.options.side) {
        let r = ws.catalog.resolve(p, &defaults);
        kinds
            .entry(p.key())
            .and_modify(|k| k.count += 1)
            .or_insert(KindRow {
                value: p.value.trim().to_string(),
                package: p.package.trim().to_string(),
                count: 1,
                height_mm: r.height_mm,
                rank: r.rank,
                tip: r.nozzle_tip,
                tape_mm: r.tape_mm,
                described: ws.catalog.is_defined(p),
            });
    }
    let mut rows: Vec<KindRow> = kinds.into_values().collect();
    rows.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| natural_cmp(&a.value, &b.value))
            .then_with(|| natural_cmp(&a.package, &b.package))
    });
    rows
}

/// A package as a row of the editable table: what its row says and what is used for it once the defaults are put in.
#[derive(Debug, Clone, PartialEq)]
pub struct PackageRow {
    pub name: String,
    /// Whether the table of packages has a row for it.
    pub defined: bool,
    /// How many parts of the board are in this package.
    pub count: usize,
    /// What the row says (nothing when it has no row).
    pub props: PartProps,
    /// The values in use: the row, else the defaults of the configuration.
    pub shown: Resolved,
}

/// The package properties with the defaults put in for those the row leaves empty.
pub fn with_defaults(p: &PartProps, d: &Resolved) -> Resolved {
    Resolved {
        height_mm: p.height_mm.unwrap_or(d.height_mm),
        through_board_mm: p.through_board_mm.unwrap_or(d.through_board_mm),
        pick_retries: p.pick_retries.unwrap_or(d.pick_retries),
        bin: p.bin.unwrap_or(d.bin),
        nozzle_tip: p.nozzle_tip.clone().unwrap_or_else(|| d.nozzle_tip.clone()),
        rank: p.rank.unwrap_or(d.rank),
        origin_dx_mm: p.origin_dx_mm.unwrap_or(d.origin_dx_mm),
        origin_dy_mm: p.origin_dy_mm.unwrap_or(d.origin_dy_mm),
        tape_mm: p.tape_mm.unwrap_or(d.tape_mm),
    }
}

/// The packages of the board together with those of the table of packages, in natural order of their names (the case of a name does not make two packages).
pub fn package_rows(ws: &Workspace) -> Vec<PackageRow> {
    let defaults = ws.cfg.part_defaults();
    let mut found: BTreeMap<String, (String, usize)> = BTreeMap::new();
    for p in &ws.parts {
        let name = p.package.trim();
        if name.is_empty() {
            continue;
        }
        found
            .entry(name.to_ascii_lowercase())
            .and_modify(|e| e.1 += 1)
            .or_insert((name.to_string(), 1));
    }
    for name in ws.catalog.package_names() {
        found.entry(name.to_ascii_lowercase()).or_insert((name, 0));
    }
    let mut rows: Vec<PackageRow> = found
        .into_values()
        .map(|(name, count)| {
            let props = ws.catalog.package(&name).cloned();
            PackageRow {
                defined: props.is_some(),
                shown: with_defaults(&props.clone().unwrap_or_default(), &defaults),
                props: props.unwrap_or_default(),
                name,
                count,
            }
        })
        .collect();
    rows.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    rows
}

/// A fiducial of the panel with the place that was measured for it and how far the fit leaves it from there.
#[derive(Debug, Clone, PartialEq)]
pub struct FiducialRow {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub measured: Option<(f64, f64)>,
    pub residual: Option<f64>,
}

/// The fiducials of the panel, in the order of the design.
pub fn fiducial_rows(ws: &Workspace) -> Vec<FiducialRow> {
    ws.design_fiducials
        .iter()
        .map(|(id, x, y)| FiducialRow {
            id: id.clone(),
            x: *x,
            y: *y,
            measured: ws.measured.iter().find(|m| m.0 == *id).map(|m| (m.1, m.2)),
            residual: ws.fit.as_ref().and_then(|f| {
                f.ids
                    .iter()
                    .position(|i| i == id)
                    .and_then(|k| f.residuals.get(k).copied())
            }),
        })
        .collect()
}

/// A board of the panel: its name, its file and how many times the panel places it.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardRow {
    pub id: String,
    pub file: String,
    pub copies: usize,
}

/// The boards of the panel that was loaded (nothing without a panel).
pub fn board_rows(ws: &Workspace) -> Vec<BoardRow> {
    let Some(panel) = &ws.panel else {
        return Vec::new();
    };
    let counts = panel.board_counts();
    panel
        .boards()
        .into_iter()
        .map(|(id, file)| BoardRow {
            copies: counts.get(&id).copied().unwrap_or(0),
            id,
            file,
        })
        .collect()
}
