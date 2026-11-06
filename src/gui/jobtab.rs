// EH3D-PHOENIX-PNP - the Job tab of the window, without the window.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! What the Job tab shows and decides, with nothing drawn: the state of every placement (waiting, picked, aligned, placed, put aside, left out, not to be mounted, failed), the filters
//! of the table, the progress with the time left, and the dialog of the error that stopped the job with the actions it offers. The window only draws these and passes the clicks back.

use super::tables::{compare_plan, directed_order, plan_rows, PlanColumn, PlanRow, SortState};
use super::texts::Text;
use super::workspace::Planned;
use crate::job::{Action, Failure, Job, Phase, Status};
use crate::model::{Machine, Nozzle};
use crate::parts::{Part, Side};

/// Where a placement is in the job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceState {
    /// Nothing was done with it yet.
    Pending,
    /// The nozzle holds it; the camera has not looked at it (or the step is off).
    Picked,
    /// The camera looked at it and the place is corrected: the head is on its way.
    Aligned,
    Placed,
    /// Put aside, by the policy or by the person.
    Deferred,
    /// Left out by the person.
    Skipped,
    /// Marked as not to be mounted: it is not in the plan.
    Dnp,
    /// The step that an error stopped is the one of this placement.
    Failed,
}

impl PlaceState {
    pub const ALL: [PlaceState; 8] = [
        PlaceState::Pending,
        PlaceState::Picked,
        PlaceState::Aligned,
        PlaceState::Placed,
        PlaceState::Deferred,
        PlaceState::Skipped,
        PlaceState::Dnp,
        PlaceState::Failed,
    ];

    /// The word of the state.
    pub fn text(self) -> Text {
        match self {
            PlaceState::Pending => Text::StatusPending,
            PlaceState::Picked => Text::StatePicked,
            PlaceState::Aligned => Text::StateAligned,
            PlaceState::Placed => Text::StatusPlaced,
            PlaceState::Deferred => Text::StatusDeferred,
            PlaceState::Skipped => Text::StateSkipped,
            PlaceState::Dnp => Text::StateDnp,
            PlaceState::Failed => Text::StateFailed,
        }
    }

    /// The place of the state in the order of a sort.
    pub fn rank(self) -> u8 {
        match self {
            PlaceState::Failed => 0,
            PlaceState::Picked => 1,
            PlaceState::Aligned => 2,
            PlaceState::Pending => 3,
            PlaceState::Deferred => 4,
            PlaceState::Skipped => 5,
            PlaceState::Placed => 6,
            PlaceState::Dnp => 7,
        }
    }

    /// True for the states in which there is nothing more to do with the placement.
    pub fn is_done(self) -> bool {
        matches!(
            self,
            PlaceState::Placed | PlaceState::Deferred | PlaceState::Skipped | PlaceState::Dnp
        )
    }
}

/// A row of the table of the Job tab.
#[derive(Debug, Clone, PartialEq)]
pub struct JobRow {
    pub row: PlanRow,
    pub state: PlaceState,
    /// Why it was put aside or left out, or the error that stopped it.
    pub note: String,
}

/// The state of the placement `index` of the plan.
pub fn place_state(job: Option<&Job>, index: usize) -> PlaceState {
    let Some(job) = job else {
        return PlaceState::Pending;
    };
    match job.status().get(index) {
        Some(Status::Placed) => PlaceState::Placed,
        Some(Status::Deferred) => PlaceState::Deferred,
        Some(Status::Skipped) => PlaceState::Skipped,
        Some(Status::Pending) | None => {
            if job.failure().is_some_and(|f| f.index == index) {
                return PlaceState::Failed;
            }
            let Some(p) = job.plan().plan.placements.get(index) else {
                return PlaceState::Pending;
            };
            if job.machine().holding(p.nozzle) == Some(p.reference.as_str()) {
                if job.correction(p.nozzle).is_some() {
                    PlaceState::Aligned
                } else {
                    PlaceState::Picked
                }
            } else {
                PlaceState::Pending
            }
        }
    }
}

/// The rows of the table: the placements in the order of the plan, each with its state, and then the parts not to be mounted of the side being worked on.
pub fn job_rows(planned: &Planned, dnp: &[Part], side: Side, job: Option<&Job>) -> Vec<JobRow> {
    let mut out: Vec<JobRow> = plan_rows(planned, job)
        .into_iter()
        .enumerate()
        .map(|(i, row)| {
            let state = place_state(job, i);
            let note = match (state, job) {
                (PlaceState::Deferred | PlaceState::Skipped, Some(j)) => j.reason(i).to_string(),
                (PlaceState::Failed, Some(j)) => {
                    j.failure().map_or(String::new(), |f| f.error.to_string())
                }
                _ => String::new(),
            };
            JobRow { row, state, note }
        })
        .collect();
    for d in dnp.iter().filter(|d| d.side == side) {
        out.push(JobRow {
            row: PlanRow {
                step: 0,
                reference: d.reference.clone(),
                value: d.value.clone(),
                feeder: String::new(),
                nozzle: 0,
                batch: 0,
                tip: String::new(),
                status: None,
                tip_change: false,
            },
            state: PlaceState::Dnp,
            note: String::new(),
        });
    }
    out
}

/// Sorts the rows by a column of the plan (the state sorts by the order of [`PlaceState::rank`]); rows that compare equal stay in the order of the plan and the parts not to be
/// mounted come after them.
pub fn sort_job(rows: &mut [JobRow], by: SortState<PlanColumn>) {
    rows.sort_by(|a, b| {
        let order = match by.column {
            PlanColumn::Status => a.state.rank().cmp(&b.state.rank()),
            column => compare_plan(&a.row, &b.row, column),
        };
        let own = (a.state == PlaceState::Dnp).cmp(&(b.state == PlaceState::Dnp));
        own.then_with(|| directed_order(order, by.ascending))
            .then_with(|| a.row.step.cmp(&b.row.step))
    });
}

/// What the table is limited to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobFilter {
    /// Only the placements in this state; none shows all.
    pub state: Option<PlaceState>,
    /// Only the placements whose reference, value or feeder holds this text (without regard to case).
    pub text: String,
    /// Leaves out the placements that are done (placed, put aside, left out, not to be mounted).
    pub hide_done: bool,
}

impl JobFilter {
    /// True when anything limits the table.
    pub fn is_active(&self) -> bool {
        self.state.is_some() || !self.text.trim().is_empty() || self.hide_done
    }

    pub fn clear(&mut self) {
        *self = JobFilter::default();
    }

    pub fn matches(&self, row: &JobRow) -> bool {
        if self.state.is_some_and(|s| s != row.state) {
            return false;
        }
        if self.hide_done && row.state.is_done() {
            return false;
        }
        let needle = self.text.trim().to_lowercase();
        needle.is_empty()
            || [&row.row.reference, &row.row.value, &row.row.feeder]
                .iter()
                .any(|field| field.to_lowercase().contains(&needle))
    }
}

/// The rows the filter lets through, in the order they are in.
pub fn filter_rows<'a>(rows: &'a [JobRow], filter: &JobFilter) -> Vec<&'a JobRow> {
    rows.iter().filter(|r| filter.matches(r)).collect()
}

/// How many rows are in each state, in the order of [`PlaceState::ALL`].
pub fn counts(rows: &[JobRow]) -> [usize; 8] {
    let mut out = [0usize; 8];
    for r in rows {
        if let Some(i) = PlaceState::ALL.iter().position(|s| *s == r.state) {
            out[i] += 1;
        }
    }
    out
}

/// How far the job is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Progress {
    /// The placements of the plan.
    pub total: usize,
    /// Those that are done: placed, put aside or left out.
    pub done: usize,
    pub placed: usize,
    /// From 0 to 1; a plan with no placement is complete.
    pub fraction: f64,
    /// The seconds of the clock of the machine so far.
    pub elapsed_s: f64,
    /// An estimate of the seconds that are left: the time per part so far times the parts left, or the time of the plan before the first part is done; 0 when the job is finished.
    pub remaining_s: f64,
}

pub fn progress(job: Option<&Job>) -> Progress {
    let Some(job) = job else {
        return Progress {
            total: 0,
            done: 0,
            placed: 0,
            fraction: 0.0,
            elapsed_s: 0.0,
            remaining_s: 0.0,
        };
    };
    let total = job.status().len();
    let done = job
        .status()
        .iter()
        .filter(|s| **s != Status::Pending)
        .count();
    let elapsed_s = job.machine().now_us() as f64 / 1e6;
    let left = total - done;
    let remaining_s = if job.phase() == Phase::Finished || left == 0 {
        0.0
    } else if done > 0 {
        elapsed_s / done as f64 * left as f64
    } else {
        job.plan().plan.seconds
    };
    Progress {
        total,
        done,
        placed: job.stats().placed,
        fraction: if total == 0 {
            1.0
        } else {
            done as f64 / total as f64
        },
        elapsed_s,
        remaining_s,
    }
}

/// The dialog of the error that stopped the job: what failed and the actions that can be taken.
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorDialog {
    pub failure: Failure,
    /// Whether each action of [`Action::ALL`] can be taken now.
    pub can: [bool; 4],
}

impl ErrorDialog {
    /// Whether the action can be taken.
    pub fn allows(&self, action: Action) -> bool {
        Action::ALL
            .iter()
            .position(|a| *a == action)
            .is_some_and(|i| self.can[i])
    }
}

/// The word on the button of an action.
pub fn action_text(action: Action) -> Text {
    match action {
        Action::Retry => Text::ActionRetry,
        Action::Skip => Text::ActionSkip,
        Action::Defer => Text::ActionDefer,
        Action::SwitchOffFeeder => Text::ActionSwitchOff,
    }
}

/// The dialog for a job stopped by an error; none when the job is not stopped. The feeder can be switched off only while it is still on.
pub fn error_dialog(job: Option<&Job>) -> Option<ErrorDialog> {
    let job = job?;
    let failure = job.failure()?;
    let feeder_on = job
        .stats()
        .feeders
        .iter()
        .find(|f| f.id == failure.feeder)
        .is_some_and(|f| !f.disabled);
    Some(ErrorDialog {
        failure,
        can: [true, true, true, feeder_on],
    })
}

/// The placement whose step is next, as an index into the plan, for the row to be marked; none when the job is finished or there is no job.
pub fn current_index(job: Option<&Job>) -> Option<usize> {
    use crate::job::Cursor;
    let job = job?;
    let (batch, k) = match job.cursor() {
        Cursor::Pick { batch, k } | Cursor::Place { batch, k } => (batch, k),
        Cursor::Finished => return None,
    };
    let mut groups: std::collections::BTreeMap<usize, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (i, p) in job.plan().plan.placements.iter().enumerate() {
        groups.entry(p.batch).or_default().push(i);
    }
    groups.values().nth(batch)?.get(k).copied()
}
