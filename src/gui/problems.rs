// EH3D-PHOENIX-PNP - the problems of a job and what can be done about them.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The list of problems: the checks before a job, each with the thing it is about and, when the program knows one, a way to put it right. A fix changes the workspace (it leaves a part
//! out of the plan or adds parts to a feeder) and can be undone; the plan is made again afterwards to see whether the problem went away.

use super::texts::{tr, Text};
use super::workspace::{Planned, Workspace};
use crate::error::{Code, Lang, PnpError};
use crate::preflight::{Severity, Source};

/// A way to put a problem right.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fix {
    /// Leaves a part that no feeder holds out of the plan.
    DropPart(String),
    /// Adds the parts a feeder is short of.
    RaiseFeeder { id: String, missing: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub severity: Severity,
    pub source: Source,
    /// The detail, in English; the words around it are in the language of the window.
    pub message: String,
    /// The error code of the program, when the problem is an error with one.
    pub code: Option<Code>,
    pub fix: Option<Fix>,
}

impl Problem {
    /// The word for the severity in a language.
    pub fn severity_text(&self, lang: Lang) -> &'static str {
        match self.severity {
            Severity::Info => tr(Text::SevInfo, lang),
            Severity::Warning => tr(Text::SevWarning, lang),
            Severity::Error => tr(Text::SevError, lang),
        }
    }

    /// What the problem is about, in a language: `part R1`, `feeder F2`.
    pub fn about(&self, lang: Lang) -> String {
        match &self.source {
            Source::Part(p) => format!("{} {p}", tr(Text::SrcPart, lang)),
            Source::Feeder(f) => format!("{} {f}", tr(Text::SrcFeeder, lang)),
            Source::Setting(s) => format!("{} {s}", tr(Text::SrcSetting, lang)),
            Source::Board => tr(Text::SrcBoard, lang).to_string(),
        }
    }

    /// The whole line: with the code and its meaning in the language when there is a code, else the detail.
    pub fn line(&self, lang: Lang) -> String {
        match self.code {
            Some(code) => PnpError::new(code, self.message.clone()).localized(lang),
            None => self.message.clone(),
        }
    }

    /// The name of the fix in a language.
    pub fn fix_text(&self, lang: Lang) -> Option<&'static str> {
        self.fix.as_ref().map(|f| match f {
            Fix::DropPart(_) => tr(Text::FixDropPart, lang),
            Fix::RaiseFeeder { .. } => tr(Text::FixRaiseFeeder, lang),
        })
    }
}

/// The problems of a plan, in the order of the checks (the errors first). A part that no feeder holds can be left out; a feeder that is short can be given the parts it lacks.
pub fn detect(planned: &Planned) -> Vec<Problem> {
    let mut out = Vec::new();
    for c in &planned.checks {
        let mut code = None;
        let mut fix = None;
        match (&c.severity, &c.source) {
            (Severity::Error, Source::Part(r)) if planned.picks.without_feeder.contains(r) => {
                fix = Some(Fix::DropPart(r.clone()));
            }
            (Severity::Error, Source::Feeder(id)) => {
                if let Some((_, n)) = planned.picks.short.iter().find(|(f, _)| f == id) {
                    fix = Some(Fix::RaiseFeeder {
                        id: id.clone(),
                        missing: *n,
                    });
                }
            }
            _ => {}
        }
        if c.severity == Severity::Error && fix.is_none() {
            code = Some(Code::PreflightFailed);
        }
        out.push(Problem {
            severity: c.severity,
            source: c.source.clone(),
            message: c.message.clone(),
            code,
            fix,
        });
    }
    out
}

/// A problem for an error that stopped the plan from being made at all.
pub fn from_error(e: &PnpError) -> Problem {
    Problem {
        severity: Severity::Error,
        source: Source::Board,
        message: e.detail.clone(),
        code: Some(e.code),
        fix: None,
    }
}

/// Puts a problem right. It leaves the state before in the undo list of the workspace; the result says whether anything changed.
pub fn apply(ws: &mut Workspace, fix: &Fix) -> bool {
    match fix {
        Fix::DropPart(reference) => {
            if ws.excluded.contains(reference)
                || !ws.parts.iter().any(|p| &p.reference == reference)
            {
                return false;
            }
            ws.remember();
            ws.excluded.insert(reference.clone());
            true
        }
        Fix::RaiseFeeder { id, missing } => {
            if *missing == 0 {
                return false;
            }
            let Some(at) = ws.feeders.iter().position(|f| &f.base.id == id) else {
                return false;
            };
            ws.remember();
            let f = &mut ws.feeders[at];
            f.base.count = f.base.count.saturating_add(*missing);
            true
        }
    }
}
