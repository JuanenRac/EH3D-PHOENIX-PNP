// EH3D-PHOENIX-PNP - the window, drawn with egui.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The drawing of the window. It holds no logic: it shows the state of `AppState` (the plan, the tables, the problems, the log, the simulated job) and passes what the person does
//! (a click, a path, a dropped file) back to it. Built with the `gui` feature.

use super::app::{fid_buffer, AppState, Tab};
use super::browse::{self, Purpose};
use super::jobtab::{self, PlaceState};
use super::log::diagnostics;
use super::problems::Fix;
use super::samples;
use super::session::Next;
use super::settings::{self, UiSettings};
use super::shot::{self, ShotRequest, PATIENCE_FRAMES};
use super::tables::{
    board_rows, feeder_rows, fiducial_rows, kind_rows, package_rows, part_rows, sort_feeders,
    sort_parts, FeederColumn, PartColumn, PlanColumn, SortState,
};
use super::texts::{lang_name, tr, Text};
use super::theme::{Palette, Rgb, ThemeId};
use super::view::{Point, Transform};
use super::wizard::Step;
use super::workspace::FileKind;
use crate::catalog::PartProps;
use crate::config::OrderStrategy;
use crate::error::Lang;
use crate::feeding::{FeederKind, TrayFill};
use crate::job::{Action, Phase, Status};
use crate::parts::Side;
use crate::preflight::Severity;
use eframe::egui::{
    self, Align2, Color32, FontData, FontDefinitions, FontFamily, FontId, Pos2, Rect, RichText,
    Sense, Stroke, StrokeKind, Vec2,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const ORDERS: [OrderStrategy; 6] = [
    OrderStrategy::Nearest,
    OrderStrategy::Part,
    OrderStrategy::HeightPart,
    OrderStrategy::Pick,
    OrderStrategy::Tips,
    OrderStrategy::None,
];

const ROW_H: f32 = 20.0;

fn col(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

fn alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

/// The colours of egui made from the theme.
fn apply_visuals(ctx: &egui::Context, pal: &Palette) {
    let mut v = if pal.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    let (panel, raised, text, accent) = (
        col(pal.panel),
        col(pal.raised),
        col(pal.text),
        col(pal.accent),
    );
    v.panel_fill = col(pal.background);
    v.window_fill = panel;
    v.extreme_bg_color = raised;
    v.override_text_color = Some(text);
    v.selection.bg_fill = alpha(accent, 110);
    v.selection.stroke = Stroke::new(1.5, accent);
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
    ] {
        w.corner_radius = 6.0.into();
        w.bg_fill = raised;
        w.weak_bg_fill = raised;
    }
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, alpha(accent, 110));
    v.widgets.hovered.bg_stroke = Stroke::new(1.5, accent);
    v.widgets.hovered.weak_bg_fill = alpha(accent, 50);
    v.widgets.active.bg_stroke = Stroke::new(2.0, accent);
    v.widgets.active.weak_bg_fill = alpha(accent, 110);
    ctx.set_visuals(v);
}

/// Fonts for the Chinese and Japanese texts, from the ones the system has.
fn install_fonts(ctx: &egui::Context) {
    let candidates = [
        "C:/Windows/Fonts/msyh.ttc",
        "C:/Windows/Fonts/YuGothM.ttc",
        "C:/Windows/Fonts/meiryo.ttc",
        "C:/Windows/Fonts/simsun.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/System/Library/Fonts/PingFang.ttc",
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = FontDefinitions::default();
            fonts
                .font_data
                .insert("cjk".into(), Arc::new(FontData::from_owned(bytes)));
            for family in [FontFamily::Monospace, FontFamily::Proportional] {
                fonts.families.entry(family).or_default().push("cjk".into());
            }
            ctx.set_fonts(fonts);
            return;
        }
    }
}

/// The icon of the program for the title bar.
fn icon() -> Option<egui::IconData> {
    let bytes: &[u8] = include_bytes!("../../images/brand/icon-256.png");
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    buf.truncate(info.buffer_size());
    Some(egui::IconData {
        rgba: buf,
        width: info.width,
        height: info.height,
    })
}

/// A cell of a table: a text in a box of a fixed width.
fn cell(ui: &mut egui::Ui, w: f32, text: impl Into<RichText>) {
    ui.add_sized([w, ROW_H - 2.0], egui::Label::new(text.into()).truncate());
}

/// The head of a table: the names of the columns over the boxes of the cells. The right edge of a name can be dragged to change the width of its column; the width is changed in
/// `widths` at once and the index of the column that was resized is given, for the caller to remember it.
fn header(
    ui: &mut egui::Ui,
    pal: &Palette,
    id: &str,
    widths: &mut [f32],
    titles: &[&str],
) -> Option<usize> {
    let mut resized = None;
    ui.horizontal(|ui| {
        for (i, t) in titles.iter().enumerate() {
            let Some(w) = widths.get(i).copied() else {
                break;
            };
            let label = egui::Label::new(RichText::new(*t).strong().color(col(pal.accent)));
            let r = ui.add_sized([w, ROW_H - 2.0], label.truncate());
            let edge = Rect::from_min_max(
                Pos2::new(r.rect.right() - 2.0, r.rect.top()),
                Pos2::new(r.rect.right() + 6.0, r.rect.bottom()),
            );
            let drag = ui.interact(edge, egui::Id::unique(id).with(i), Sense::drag());
            if drag.hovered() || drag.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            if drag.dragged() {
                widths[i] = (w + drag.drag_delta().x).clamp(30.0, 600.0);
                resized = Some(i);
            }
        }
    });
    resized
}

/// A row of a table that can be chosen: the row is painted behind its cells when it is chosen, and the answer says whether it was clicked.
fn row_ui(
    ui: &mut egui::Ui,
    id: egui::Id,
    chosen: bool,
    pal: &Palette,
    sense: bool,
    add: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let back = ui.painter().add(egui::Shape::Noop);
    let inner = ui.horizontal(add);
    let rect = inner.response.rect;
    if chosen {
        ui.painter().set(
            back,
            egui::Shape::rect_filled(rect, 2.0, alpha(col(pal.accent), 60)),
        );
    }
    let s = if sense {
        Sense::click()
    } else {
        Sense::hover()
    };
    ui.interact(rect, id, s)
}

/// The box that chooses how a table is sorted: the column and the direction.
fn sort_picker<C: Copy + PartialEq>(
    ui: &mut egui::Ui,
    id: &str,
    options: &[(C, Text)],
    sort: &mut SortState<C>,
    lang: Lang,
) {
    let current = options
        .iter()
        .find(|(c, _)| *c == sort.column)
        .map_or("", |(_, t)| tr(*t, lang));
    egui::ComboBox::from_id_salt(id)
        .selected_text(current)
        .show_ui(ui, |ui| {
            for (c, t) in options {
                ui.selectable_value(&mut sort.column, *c, tr(*t, lang));
            }
        });
    let arrow = if sort.ascending { "^" } else { "v" };
    if ui.small_button(arrow).clicked() {
        sort.ascending = !sort.ascending;
    }
}

fn status_label(status: Option<Status>, lang: Lang, pal: &Palette) -> (&'static str, Color32) {
    match status {
        None => ("", col(pal.dim)),
        Some(Status::Pending) => (tr(Text::StatusPending, lang), col(pal.dim)),
        Some(Status::Placed) => (tr(Text::StatusPlaced, lang), col(pal.ok)),
        Some(Status::Deferred) => (tr(Text::StatusDeferred, lang), col(pal.warn)),
        Some(Status::Skipped) => (tr(Text::StateSkipped, lang), col(pal.warn)),
    }
}

/// The colour of the state of a placement in the Job tab.
fn state_color(state: PlaceState, pal: &Palette) -> Color32 {
    match state {
        PlaceState::Pending | PlaceState::Dnp => col(pal.dim),
        PlaceState::Picked => col(pal.accent),
        PlaceState::Aligned => col(pal.accent2),
        PlaceState::Placed => col(pal.ok),
        PlaceState::Deferred | PlaceState::Skipped => col(pal.warn),
        PlaceState::Failed => col(pal.danger),
    }
}

fn severity_color(s: Severity, pal: &Palette) -> Color32 {
    match s {
        Severity::Info => col(pal.dim),
        Severity::Warning => col(pal.warn),
        Severity::Error => col(pal.danger),
    }
}

fn tab_for(step: Step) -> Tab {
    match step {
        Step::Parts => Tab::Parts,
        Step::Feeders => Tab::Feeders,
        Step::Machine => Tab::Machine,
        Step::Check => Tab::Problems,
        Step::Simulate => Tab::Job,
    }
}

pub struct PnpApp {
    s: AppState,
    visuals_for: Option<ThemeId>,
    saved: UiSettings,
    /// The picture mode, while the window is there only to be photographed.
    shot: Option<ShotState>,
}

impl PnpApp {
    pub fn new(ui: UiSettings, paths: &[PathBuf]) -> PnpApp {
        let saved = ui.clone();
        let mut s = AppState::new(ui);
        if !paths.is_empty() {
            s.load_paths(paths);
        }
        PnpApp {
            s,
            visuals_for: None,
            saved,
            shot: None,
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let pal = self.s.ui.theme.palette();
        let lang = self.s.lang();
        egui::Panel::top("bar").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("EH3D-PHOENIX-PNP")
                        .strong()
                        .color(col(pal.accent)),
                );
                ui.separator();
                ui.menu_button(tr(Text::Sample, lang), |ui| {
                    if ui.button(tr(Text::SampleMini, lang)).clicked() {
                        self.s.load_sample(&samples::mini());
                        ui.close();
                    }
                    if ui.button(tr(Text::SampleFiles, lang)).clicked() {
                        self.s.load_sample(&samples::examples());
                        ui.close();
                    }
                    if ui.button(tr(Text::SampleLarge, lang)).clicked() {
                        self.s.load_sample(&samples::large(400));
                        ui.close();
                    }
                });
                let can_plan = self.s.ws.can_plan();
                if ui
                    .add_enabled(can_plan, egui::Button::new(tr(Text::Plan, lang)))
                    .clicked()
                {
                    self.s.replan();
                }
                ui.separator();
                let phase = self.s.session.phase();
                let resumable = matches!(phase, Some(Phase::Paused | Phase::Stopped));
                let start_text = if resumable {
                    tr(Text::Resume, lang)
                } else {
                    tr(Text::Start, lang)
                };
                let can_run = self.s.can_start() && phase != Some(Phase::Finished);
                if ui
                    .add_enabled(can_run, egui::Button::new(start_text))
                    .clicked()
                {
                    self.s.start();
                }
                if ui
                    .add_enabled(
                        self.s.session.is_running(),
                        egui::Button::new(tr(Text::Pause, lang)),
                    )
                    .clicked()
                {
                    self.s.pause();
                }
                let can_step = (self.s.can_start() || self.s.session.has_job())
                    && phase != Some(Phase::Finished);
                if ui
                    .add_enabled(can_step, egui::Button::new(tr(Text::Step, lang)))
                    .clicked()
                {
                    self.s.step();
                }
                if ui
                    .add_enabled(
                        self.s.session.has_job(),
                        egui::Button::new(tr(Text::Reset, lang)),
                    )
                    .clicked()
                {
                    self.s.reset_job();
                }
                ui.separator();
                let mut chosen = lang;
                egui::ComboBox::from_id_salt("language")
                    .selected_text(lang_name(lang))
                    .show_ui(ui, |ui| {
                        for l in Lang::ALL {
                            ui.selectable_value(&mut chosen, l, lang_name(l));
                        }
                    });
                if chosen != lang {
                    self.s.set_lang(chosen);
                }
                let mut theme = self.s.ui.theme;
                egui::ComboBox::from_id_salt("theme")
                    .selected_text(tr(theme.text(), lang))
                    .show_ui(ui, |ui| {
                        for id in ThemeId::ALL {
                            ui.selectable_value(&mut theme, id, tr(id.text(), lang));
                        }
                    });
                if theme != self.s.ui.theme {
                    self.s.set_theme(theme);
                }
                if ui
                    .checkbox(&mut self.s.wizard.open, tr(Text::TabGuide, lang))
                    .changed()
                {
                    self.s.ui.guide_seen = true;
                }
            });
        });
    }

    fn path_bar(&mut self, ui: &mut egui::Ui) {
        let lang = self.s.lang();
        egui::Panel::top("paths").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr(Text::Load, lang));
                let width = (ui.available_width() - 640.0).max(120.0);
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.s.path_text)
                        .desired_width(width)
                        .hint_text(tr(Text::LoadHint, lang)),
                );
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button(tr(Text::Browse, lang)).clicked() {
                    self.s.open_browser(Purpose::Files);
                }
                if ui.button(tr(Text::Load, lang)).clicked() || enter {
                    let paths: Vec<PathBuf> = self
                        .s
                        .path_text
                        .split(';')
                        .map(|p| p.trim().trim_matches('"'))
                        .filter(|p| !p.is_empty())
                        .map(PathBuf::from)
                        .collect();
                    if !paths.is_empty() {
                        self.s.load_paths(&paths);
                        self.s.path_text.clear();
                    }
                }
                ui.separator();
                ui.label(tr(Text::OutFolder, lang));
                ui.add(
                    egui::TextEdit::singleline(&mut self.s.ui.out_dir)
                        .desired_width(160.0)
                        .hint_text("."),
                );
                if ui.button(tr(Text::Browse, lang)).clicked() {
                    self.s.open_browser(Purpose::Folder);
                }
                let can_save = self.s.planned.is_some();
                if ui
                    .add_enabled(can_save, egui::Button::new(tr(Text::SaveFiles, lang)))
                    .clicked()
                {
                    if let Err(e) = self.s.save_outputs() {
                        self.s.log.error(e);
                    }
                }
            });
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let pal = self.s.ui.theme.palette();
        let lang = self.s.lang();
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                let (text, color) = match self.s.session.phase() {
                    None => (tr(Text::PhaseNoJob, lang), col(pal.dim)),
                    Some(Phase::Ready) => {
                        if self.s.session.is_running() {
                            (tr(Text::PhaseRunning, lang), col(pal.ok))
                        } else {
                            (tr(Text::PhaseReady, lang), col(pal.accent))
                        }
                    }
                    Some(Phase::Paused) => (tr(Text::PhasePaused, lang), col(pal.warn)),
                    Some(Phase::Stopped) => (tr(Text::PhaseStopped, lang), col(pal.danger)),
                    Some(Phase::Finished) => (tr(Text::PhaseFinished, lang), col(pal.ok)),
                };
                ui.label(RichText::new(text).strong().color(color));
                let (done, total) = self.s.session.progress();
                if total > 0 {
                    ui.add(
                        egui::ProgressBar::new(done as f32 / total as f32)
                            .desired_width(220.0)
                            .text(format!("{done}/{total}")),
                    );
                    ui.label(format!(
                        "{:.1} s {}",
                        self.s.session.machine_seconds(),
                        tr(Text::SumMachineTime, lang)
                    ));
                }
                if let Some(e) = self.s.session.last_error() {
                    ui.label(RichText::new(e.localized(lang)).color(col(pal.danger)));
                }
            });
        });
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        let pal = self.s.ui.theme.palette();
        let lang = self.s.lang();
        egui::Panel::left("side")
            .default_size(270.0)
            .resizable(true)
            .show(ui, |ui| {
                ui.add_space(4.0);
                if self.s.wizard.open {
                    let step = self.s.wizard.step;
                    ui.label(
                        RichText::new(tr(step.title(), lang))
                            .strong()
                            .color(col(pal.accent)),
                    );
                    ui.label(tr(step.hint(), lang));
                    ui.add(
                        egui::ProgressBar::new((step.index() + 1) as f32 / Step::ALL.len() as f32)
                            .desired_width(200.0),
                    );
                    let mut moved = false;
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                step.prev().is_some(),
                                egui::Button::new(tr(Text::Back, lang)),
                            )
                            .clicked()
                        {
                            moved = self.s.wizard.back();
                        }
                        let state = self.s.plan_state();
                        let can =
                            self.s.wizard.can_continue(&self.s.ws, state) && step.next().is_some();
                        if ui
                            .add_enabled(can, egui::Button::new(tr(Text::Next, lang)))
                            .clicked()
                        {
                            moved = self.s.wizard.next(&self.s.ws, state);
                        }
                    });
                    if moved {
                        self.s.tab = tab_for(self.s.wizard.step);
                    }
                    ui.separator();
                }
                ui.label(
                    RichText::new(tr(Text::LoadedFiles, lang))
                        .strong()
                        .color(col(pal.accent)),
                );
                if self.s.ws.names.is_empty() {
                    ui.label(RichText::new(tr(Text::NothingLoaded, lang)).color(col(pal.dim)));
                }
                for kind in FileKind::ALL {
                    if let Some(name) = self.s.ws.names.get(&kind) {
                        ui.label(format!("{}: {name}", kind.key()));
                    }
                }
                ui.add_space(8.0);
                ui.label(RichText::new(tr(Text::DropHere, lang)).color(col(pal.dim)));
            });
    }

    fn center(&mut self, ui: &mut egui::Ui) {
        let lang = self.s.lang();
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for tab in Tab::ALL {
                    let mut label = tr(tab.text(), lang).to_string();
                    if tab == Tab::Problems && !self.s.problems.is_empty() {
                        label = format!("{label} ({})", self.s.problems.len());
                    }
                    if ui.selectable_label(self.s.tab == tab, label).clicked() {
                        self.s.tab = tab;
                    }
                }
            });
            ui.separator();
            match self.s.tab {
                Tab::Job => self.job_tab(ui),
                Tab::Parts => self.parts_tab(ui),
                Tab::Packages => self.packages_tab(ui),
                Tab::Feeders => self.feeders_tab(ui),
                Tab::Panels => self.panels_tab(ui),
                Tab::Boards => self.boards_tab(ui),
                Tab::LoadSheet => self.load_sheet_tab(ui),
                Tab::Machine => self.machine_tab(ui),
                Tab::Problems => self.problems_tab(ui),
                Tab::Log => self.log_tab(ui),
            }
        });
    }

    fn job_tab(&mut self, ui: &mut egui::Ui) {
        let pal = self.s.ui.theme.palette();
        let lang = self.s.lang();
        match self.s.summary() {
            Some(m) => {
                ui.label(format!(
                    "{} {} | {} {} | {} {} | {:.0} mm {} | {:.0} s {}",
                    m.parts,
                    tr(Text::SumParts, lang),
                    m.batches,
                    tr(Text::SumBatches, lang),
                    m.tip_changes,
                    tr(Text::SumTipChanges, lang),
                    m.travel_mm,
                    tr(Text::SumTravel, lang),
                    m.seconds,
                    tr(Text::SumTime, lang)
                ));
            }
            None => {
                ui.label(RichText::new(tr(Text::NoPlan, lang)).color(col(pal.dim)));
            }
        }
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.s.ui.layers.parts, tr(Text::LayerParts, lang));
            ui.checkbox(&mut self.s.ui.layers.feeders, tr(Text::LayerFeeders, lang));
            ui.checkbox(&mut self.s.ui.layers.path, tr(Text::LayerPath, lang));
            ui.checkbox(
                &mut self.s.ui.layers.polarity,
                tr(Text::LayerPolarity, lang),
            );
            let moved = !self.s.view.is_default();
            if ui
                .add_enabled(moved, egui::Button::new(tr(Text::FitView, lang)))
                .clicked()
            {
                self.s.view.reset();
            }
        });
        let map_h = (ui.available_height() * 0.55).max(180.0);
        self.paint_map(ui, map_h);
        ui.separator();
        let side = self.s.ws.options.side;
        let mut rows = match self.s.planned.as_ref() {
            Some(planned) => jobtab::job_rows(planned, &self.s.ws.dnp, side, self.s.session.job()),
            None => return,
        };
        let prog = jobtab::progress(self.s.session.job());
        let counts = jobtab::counts(&rows);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(tr(Text::ProgressLabel, lang))
                    .strong()
                    .color(col(pal.accent)),
            );
            ui.add(
                egui::ProgressBar::new(prog.fraction as f32)
                    .desired_width(260.0)
                    .text(format!(
                        "{}/{} ({:.0} %)",
                        prog.done,
                        prog.total,
                        prog.fraction * 100.0
                    )),
            );
            if let Some(job) = self.s.session.job() {
                ui.label(format!(
                    "{:.1} s {}",
                    prog.elapsed_s,
                    tr(Text::SumMachineTime, lang)
                ));
                ui.label(format!(
                    "{:.0} s {}",
                    prog.remaining_s,
                    tr(Text::RemainingLabel, lang)
                ));
                ui.label(format!(
                    "{:.0} {}",
                    job.parts_per_hour(),
                    tr(Text::PartsPerHour, lang)
                ));
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (state, n) in PlaceState::ALL.iter().zip(counts) {
                if n > 0 {
                    ui.label(
                        RichText::new(format!("{}: {n}", tr(state.text(), lang)))
                            .color(state_color(*state, &pal)),
                    );
                }
            }
        });
        ui.horizontal(|ui| {
            let current = match self.s.job_filter.state {
                Some(state) => tr(state.text(), lang),
                None => tr(Text::FilterAllStates, lang),
            };
            egui::ComboBox::from_id_salt("job_state_filter")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.s.job_filter.state,
                        None,
                        tr(Text::FilterAllStates, lang),
                    );
                    for state in PlaceState::ALL {
                        ui.selectable_value(
                            &mut self.s.job_filter.state,
                            Some(state),
                            tr(state.text(), lang),
                        );
                    }
                });
            ui.add(
                egui::TextEdit::singleline(&mut self.s.job_filter.text)
                    .desired_width(200.0)
                    .hint_text(tr(Text::FilterHint, lang)),
            );
            ui.checkbox(&mut self.s.job_filter.hide_done, tr(Text::HideDone, lang));
            if ui
                .add_enabled(
                    self.s.job_filter.is_active(),
                    egui::Button::new(tr(Text::ClearFilter, lang)),
                )
                .clicked()
            {
                self.s.job_filter.clear();
            }
            if ui
                .add_enabled(
                    self.s.session.has_job(),
                    egui::Button::new(tr(Text::CopyReport, lang)),
                )
                .clicked()
            {
                if let Some(text) = self.s.report_text() {
                    ui.ctx().copy_text(text);
                }
            }
        });
        let mut sort = self.s.plan_sort;
        ui.horizontal(|ui| {
            sort_picker(
                ui,
                "plan_sort",
                &[
                    (PlanColumn::Step, Text::Step),
                    (PlanColumn::Reference, Text::ColRef),
                    (PlanColumn::Value, Text::ColValue),
                    (PlanColumn::Feeder, Text::ColFeeder),
                    (PlanColumn::Nozzle, Text::ColNozzle),
                    (PlanColumn::Batch, Text::ColBatch),
                    (PlanColumn::Tip, Text::ColTip),
                    (PlanColumn::Status, Text::ColStatus),
                ],
                &mut sort,
                lang,
            );
        });
        self.s.plan_sort = sort;
        jobtab::sort_job(&mut rows, sort);
        let filter = self.s.job_filter.clone();
        let shown = jobtab::filter_rows(&rows, &filter);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "{}/{} {}",
                    shown.len(),
                    rows.len(),
                    tr(Text::ShownOf, lang)
                ))
                .color(col(pal.dim)),
            );
            if !self.s.session.has_job() {
                ui.label(RichText::new(tr(Text::NoJobYet, lang)).color(col(pal.dim)));
            }
        });
        let mut widths = self
            .s
            .ui
            .column_widths("plan", &[44.0, 70.0, 100.0, 70.0, 56.0, 56.0, 80.0, 90.0]);
        if let Some(i) = header(
            ui,
            &pal,
            "plan_head",
            &mut widths,
            &[
                "#",
                tr(Text::ColRef, lang),
                tr(Text::ColValue, lang),
                tr(Text::ColFeeder, lang),
                tr(Text::ColNozzle, lang),
                tr(Text::ColBatch, lang),
                tr(Text::ColTip, lang),
                tr(Text::ColStatus, lang),
            ],
        ) {
            self.s.ui.set_column_width("plan", i, widths[i]);
        }
        let mut clicked: Option<String> = None;
        let next = jobtab::current_index(self.s.session.job());
        egui::ScrollArea::vertical()
            .id_salt("plan_rows")
            .auto_shrink([false, false])
            .show_rows(ui, ROW_H, shown.len(), |ui, range| {
                for i in range {
                    let r = shown[i];
                    let chosen = self.s.selection.is_part(&r.row.reference);
                    let current = r.row.step > 0 && next == Some(r.row.step - 1);
                    let number = |n: usize| {
                        if n == 0 {
                            "-".to_string()
                        } else {
                            n.to_string()
                        }
                    };
                    let resp = row_ui(
                        ui,
                        egui::Id::unique("plan_row").with(r.row.step).with(i),
                        chosen || current,
                        &pal,
                        true,
                        |ui| {
                            cell(ui, widths[0], number(r.row.step));
                            cell(ui, widths[1], r.row.reference.clone());
                            cell(ui, widths[2], r.row.value.clone());
                            cell(ui, widths[3], r.row.feeder.clone());
                            cell(ui, widths[4], number(r.row.nozzle));
                            cell(ui, widths[5], number(r.row.batch));
                            let tip = if r.row.tip_change {
                                RichText::new(format!("{} *", r.row.tip)).color(col(pal.warn))
                            } else {
                                RichText::new(r.row.tip.clone())
                            };
                            cell(ui, widths[6], tip);
                            let state = ui.add_sized(
                                [widths[7], ROW_H - 2.0],
                                egui::Label::new(
                                    RichText::new(tr(r.state.text(), lang))
                                        .color(state_color(r.state, &pal)),
                                )
                                .truncate(),
                            );
                            if !r.note.is_empty() {
                                state.on_hover_text(r.note.as_str());
                            }
                        },
                    );
                    if resp.clicked() {
                        clicked = Some(r.row.reference.clone());
                    }
                }
            });
        if let Some(r) = clicked {
            self.s.selection.select_part(&r, self.s.planned.as_ref());
        }
    }

    /// The dialog of the error that stopped the job: what failed and what the person can do about it. The action chosen is carried out on the simulated job, which then goes on.
    fn error_window(&mut self, ctx: &egui::Context) {
        let Some(dialog) = jobtab::error_dialog(self.s.session.job()) else {
            return;
        };
        let pal = self.s.ui.theme.palette();
        let lang = self.s.lang();
        let f = &dialog.failure;
        let mut chosen: Option<Action> = None;
        egui::Window::new(tr(Text::ErrorTitle, lang))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(RichText::new(f.error.localized(lang)).color(col(pal.danger)));
                ui.add_space(4.0);
                let step = if f.at_pick {
                    tr(Text::StepPick, lang)
                } else {
                    tr(Text::StepPlace, lang)
                };
                ui.label(format!(
                    "{}: {}   {}: {}   {}: {}   {}: {step}",
                    tr(Text::ColRef, lang),
                    f.reference,
                    tr(Text::ColFeeder, lang),
                    f.feeder,
                    tr(Text::ColNozzle, lang),
                    f.nozzle + 1,
                    tr(Text::FailedStep, lang)
                ));
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    for action in Action::ALL {
                        if ui
                            .add_enabled(
                                dialog.allows(action),
                                egui::Button::new(tr(jobtab::action_text(action), lang)),
                            )
                            .clicked()
                        {
                            chosen = Some(action);
                        }
                    }
                });
            });
        if let Some(action) = chosen {
            self.s.decide(action);
        }
    }

    /// The work table seen from above: the parts at their places, the feeders and the path of the head, with the part that is next marked.
    fn paint_map(&mut self, ui: &mut egui::Ui, height: f32) {
        let pal = self.s.ui.theme.palette();
        let lang = self.s.lang();
        let layers = self.s.ui.layers;
        let size = Vec2::new(ui.available_width().max(100.0), height);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 8.0, col(pal.panel));
        let Some(scene) = self.s.scene.as_ref() else {
            p.text(
                rect.center(),
                Align2::CENTER_CENTER,
                tr(Text::NoPlan, lang),
                FontId::proportional(14.0),
                col(pal.dim),
            );
            return;
        };
        if resp.dragged() {
            let d = resp.drag_delta();
            self.s.view.drag(f64::from(d.x), f64::from(d.y));
        }
        let centre = rect.center();
        if resp.hovered() {
            let wheel = ui.input(|i| i.smooth_scroll_delta.y);
            if wheel != 0.0 {
                if let Some(h) = resp.hover_pos() {
                    self.s.view.wheel(
                        f64::from(wheel),
                        (f64::from(h.x - centre.x), f64::from(h.y - centre.y)),
                    );
                }
            }
        }
        if resp.double_clicked() {
            self.s.view.reset();
        }
        let (w, h) = (f64::from(rect.width()), f64::from(rect.height()));
        let tf = Transform::new(&scene.bounds, (w, h), 28.0, &self.s.view);
        let k = tf.scale();
        let at = |q: Point| {
            let s = tf.to_screen(q);
            Pos2::new(centre.x + s.0 as f32, centre.y + s.1 as f32)
        };
        // the grid, as fine as the zoom allows
        let steps = [1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0];
        let step = steps
            .iter()
            .copied()
            .find(|s| s * k >= 40.0)
            .unwrap_or(1000.0);
        let low = tf.to_world((-w / 2.0, h / 2.0));
        let high = tf.to_world((w / 2.0, -h / 2.0));
        let grid = Stroke::new(1.0, col(pal.grid));
        let mut x = (low.0 / step).floor() * step;
        let mut n = 0;
        while x <= high.0 && n < 400 {
            p.line_segment([at((x, low.1)), at((x, high.1))], grid);
            x += step;
            n += 1;
        }
        let mut y = (low.1 / step).floor() * step;
        n = 0;
        while y <= high.1 && n < 400 {
            p.line_segment([at((low.0, y)), at((high.0, y))], grid);
            y += step;
            n += 1;
        }
        let axis = Stroke::new(1.5, alpha(col(pal.dim), 160));
        p.line_segment([at((low.0, 0.0)), at((high.0, 0.0))], axis);
        p.line_segment([at((0.0, low.1)), at((0.0, high.1))], axis);
        if layers.feeders {
            for f in &scene.feeders {
                let c = at(f.at);
                let colour = if f.enabled {
                    col(pal.feeder)
                } else {
                    alpha(col(pal.feeder), 70)
                };
                p.circle_filled(c, 5.0, colour);
                if self.s.selection.is_feeder(&f.id) {
                    p.circle_stroke(c, 9.0, Stroke::new(2.0, col(pal.accent2)));
                }
                p.text(
                    c + Vec2::new(8.0, -8.0),
                    Align2::LEFT_BOTTOM,
                    &f.id,
                    FontId::proportional(11.0),
                    col(pal.dim),
                );
            }
        }
        if layers.path {
            let colours = [
                pal.path,
                pal.accent,
                pal.feeder,
                pal.warn,
                pal.accent2,
                pal.danger,
            ];
            for pair in scene.path.windows(2) {
                if pair[0].1 == pair[1].1 {
                    let c = col(colours[pair[0].1 % colours.len()]);
                    p.line_segment(
                        [at(pair[0].0), at(pair[1].0)],
                        Stroke::new(1.2, alpha(c, 150)),
                    );
                }
            }
        }
        let job = self.s.session.job();
        let next = self.s.session.next();
        if layers.parts {
            for sh in &scene.shapes {
                let status = job.and_then(|j| j.status().get(sh.index).copied());
                let fill = match status {
                    Some(Status::Placed) => alpha(col(pal.ok), 200),
                    Some(Status::Deferred) => alpha(col(pal.warn), 200),
                    Some(Status::Skipped) => alpha(col(pal.warn), 110),
                    _ => alpha(col(pal.accent), 70),
                };
                let chosen = self.s.selection.is_part(&sh.reference);
                let is_next = matches!(next, Next::Pick(i) | Next::Place(i) if i == sh.index);
                let stroke = if chosen {
                    Stroke::new(2.5, col(pal.accent2))
                } else if is_next {
                    Stroke::new(2.0, col(pal.warn))
                } else {
                    Stroke::new(1.0, col(pal.accent))
                };
                let px = (sh.size.0.max(sh.size.1) * k) as f32;
                if px < 5.0 {
                    let c = at(sh.at);
                    p.circle_filled(c, 2.5, fill);
                    if chosen || is_next {
                        p.circle_stroke(c, 5.0, stroke);
                    }
                } else {
                    let outline: Vec<Pos2> = sh.outline.iter().map(|q| at(*q)).collect();
                    p.add(egui::Shape::convex_polygon(outline, fill, stroke));
                    if layers.polarity && px >= 10.0 {
                        let mark: Vec<Pos2> = sh.mark.iter().map(|q| at(*q)).collect();
                        p.add(egui::Shape::convex_polygon(
                            mark,
                            col(pal.warn),
                            Stroke::NONE,
                        ));
                    }
                }
                if chosen {
                    p.text(
                        at(sh.at) + Vec2::new(8.0, -8.0),
                        Align2::LEFT_BOTTOM,
                        &sh.reference,
                        FontId::proportional(12.0),
                        col(pal.text),
                    );
                }
            }
        }
        if let Some(head) = self.s.session.head() {
            let c = at(head);
            let cross = Stroke::new(1.5, col(pal.danger));
            p.circle_stroke(c, 8.0, cross);
            p.line_segment([c - Vec2::new(13.0, 0.0), c + Vec2::new(13.0, 0.0)], cross);
            p.line_segment([c - Vec2::new(0.0, 13.0), c + Vec2::new(0.0, 13.0)], cross);
        }
        p.rect_stroke(
            rect,
            8.0,
            Stroke::new(1.0, col(pal.line)),
            StrokeKind::Inside,
        );
        p.text(
            rect.right_bottom() - Vec2::new(10.0, 8.0),
            Align2::RIGHT_BOTTOM,
            format!(
                "{:.0} x {:.0} mm",
                scene.bounds.width(),
                scene.bounds.height()
            ),
            FontId::proportional(12.0),
            col(pal.dim),
        );
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let rel = (f64::from(pos.x - centre.x), f64::from(pos.y - centre.y));
                let world = tf.to_world(rel);
                let hit = scene
                    .hit(world, 8.0 / k.max(1e-6))
                    .and_then(|i| scene.shapes.get(i))
                    .map(|sh| sh.reference.clone());
                match hit {
                    Some(r) => self.s.selection.select_part(&r, self.s.planned.as_ref()),
                    None => self.s.selection.clear(),
                }
            }
        }
    }

    fn parts_tab(&mut self, ui: &mut egui::Ui) {
        let pal = self.s.ui.theme.palette();
        let lang = self.s.lang();
        let mut rows = part_rows(&self.s.ws, self.s.planned.as_ref(), self.s.session.job());
        let mut sort = self.s.parts_sort;
        ui.horizontal(|ui| {
            ui.label(format!("{} {}", rows.len(), tr(Text::SumParts, lang)));
            sort_picker(
                ui,
                "parts_sort",
                &[
                    (PartColumn::Reference, Text::ColRef),
                    (PartColumn::Value, Text::ColValue),
                    (PartColumn::Package, Text::ColPackage),
                    (PartColumn::X, Text::ColX),
                    (PartColumn::Y, Text::ColY),
                    (PartColumn::Rotation, Text::ColRot),
                    (PartColumn::Side, Text::ColSide),
                    (PartColumn::Feeder, Text::ColFeeder),
                    (PartColumn::Status, Text::ColStatus),
                ],
                &mut sort,
                lang,
            );
        });
        self.s.parts_sort = sort;
        sort_parts(&mut rows, sort);
        let mut widths = self.s.ui.column_widths(
            "parts",
            &[70.0, 100.0, 120.0, 70.0, 70.0, 60.0, 70.0, 70.0, 90.0],
        );
        if let Some(i) = header(
            ui,
            &pal,
            "parts_head",
            &mut widths,
            &[
                tr(Text::ColRef, lang),
                tr(Text::ColValue, lang),
                tr(Text::ColPackage, lang),
                tr(Text::ColX, lang),
                tr(Text::ColY, lang),
                tr(Text::ColRot, lang),
                tr(Text::ColSide, lang),
                tr(Text::ColFeeder, lang),
                tr(Text::ColStatus, lang),
            ],
        ) {
            self.s.ui.set_column_width("parts", i, widths[i]);
        }
        let mut clicked: Option<String> = None;
        egui::ScrollArea::vertical()
            .id_salt("part_rows")
            .auto_shrink([false, false])
            .show_rows(ui, ROW_H, rows.len(), |ui, range| {
                for i in range {
                    let r = &rows[i];
                    let chosen = self.s.selection.is_part(&r.reference);
                    let (status_text, status_color) = status_label(r.status, lang, &pal);
                    let side = if r.side == Side::Top {
                        tr(Text::SideTop, lang)
                    } else {
                        tr(Text::SideBottom, lang)
                    };
                    let dim = r.excluded;
                    let tint = |t: RichText| {
                        if dim {
                            t.color(col(pal.dim)).strikethrough()
                        } else {
                            t
                        }
                    };
                    let resp = row_ui(
                        ui,
                        egui::Id::unique("part_row").with(i),
                        chosen,
                        &pal,
                        true,
                        |ui| {
                            cell(ui, widths[0], tint(RichText::new(r.reference.clone())));
                            cell(ui, widths[1], tint(RichText::new(r.value.clone())));
                            cell(ui, widths[2], tint(RichText::new(r.package.clone())));
                            cell(ui, widths[3], format!("{:.3}", r.x));
                            cell(ui, widths[4], format!("{:.3}", r.y));
                            cell(ui, widths[5], format!("{:.1}", r.rotation));
                            cell(ui, widths[6], side);
                            cell(ui, widths[7], r.feeder.clone());
                            cell(
                                ui,
                                widths[8],
                                RichText::new(status_text).color(status_color),
                            );
                        },
                    );
                    if resp.clicked() {
                        clicked = Some(r.reference.clone());
                    }
                }
            });
        if let Some(r) = clicked {
            self.s.selection.select_part(&r, self.s.planned.as_ref());
        }
    }

    fn packages_tab(&mut self, ui: &mut egui::Ui) {
        let pal = self.s.ui.theme.palette();
        let lang = self.s.lang();
        let rows = package_rows(&self.s.ws);
        let mut add = false;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(tr(Text::TabPackages, lang))
                    .strong()
                    .color(col(pal.accent)),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.s.new_package)
                    .desired_width(160.0)
                    .hint_text(tr(Text::NewPackageHint, lang)),
            );
            add = ui.button(tr(Text::AddPackage, lang)).clicked();
            if ui
                .add_enabled(
                    self.s.ws.can_undo(),
                    egui::Button::new(tr(Text::Undo, lang)),
                )
                .clicked()
            {
                self.s.undo();
            }
        });
        if add {
            let name = self.s.new_package.clone();
            if self.s.add_package(&name) {
                self.s.new_package.clear();
            }
        }
        ui.label(RichText::new(tr(Text::EditedNote, lang)).color(col(pal.dim)));
        let mut widths = self.s.ui.column_widths(
            "pkgs",
            &[130.0, 50.0, 80.0, 60.0, 100.0, 80.0, 50.0, 70.0, 30.0],
        );
        if let Some(i) = header(
            ui,
            &pal,
            "pkgs_head",
            &mut widths,
            &[
                tr(Text::ColPackage, lang),
                tr(Text::ColKinds, lang),
                tr(Text::ColHeight, lang),
                tr(Text::ColRank, lang),
                tr(Text::ColTip, lang),
                tr(Text::ColTape, lang),
                tr(Text::ColBin, lang),
                tr(Text::ColRetries, lang),
                "",
            ],
        ) {
            self.s.ui.set_column_width("pkgs", i, widths[i]);
        }
        let mut pending: Option<(String, PartProps)> = None;
        let mut remove: Option<String> = None;
        let height = (ui.available_height() * 0.55).max(120.0);
        egui::ScrollArea::vertical()
            .id_salt("package_rows")
            .max_height(height)
            .auto_shrink([false, false])
            .show_rows(ui, ROW_H, rows.len(), |ui, range| {
                for i in range {
                    let r = &rows[i];
                    let mut props = r.props.clone();
                    let mut changed = false;
                    ui.horizontal(|ui| {
                        let name = if r.defined {
                            RichText::new(r.name.clone())
                        } else {
                            RichText::new(format!("{} !", r.name)).color(col(pal.warn))
                        };
                        cell(ui, widths[0], name);
                        cell(ui, widths[1], r.count.to_string());
                        let mut h = r.shown.height_mm;
                        if ui
                            .add_sized(
                                [widths[2], ROW_H - 2.0],
                                egui::DragValue::new(&mut h)
                                    .range(0.0..=100.0)
                                    .speed(0.05)
                                    .max_decimals(2),
                            )
                            .changed()
                        {
                            props.height_mm = Some(h);
                            changed = true;
                        }
                        let mut rank = r.shown.rank;
                        if ui
                            .add_sized(
                                [widths[3], ROW_H - 2.0],
                                egui::DragValue::new(&mut rank).range(-1000..=1000),
                            )
                            .changed()
                        {
                            props.rank = Some(rank);
                            changed = true;
                        }
                        let mut tip = r.shown.nozzle_tip.clone();
                        if ui
                            .add_sized(
                                [widths[4], ROW_H - 2.0],
                                egui::TextEdit::singleline(&mut tip),
                            )
                            .changed()
                        {
                            let tip: String = tip.chars().take(64).collect();
                            props.nozzle_tip = (!tip.is_empty()).then_some(tip);
                            changed = true;
                        }
                        let mut tape = r.shown.tape_mm;
                        if ui
                            .add_sized(
                                [widths[5], ROW_H - 2.0],
                                egui::DragValue::new(&mut tape)
                                    .range(1.0..=100.0)
                                    .speed(0.1)
                                    .max_decimals(1),
                            )
                            .changed()
                        {
                            props.tape_mm = Some(tape);
                            changed = true;
                        }
                        let mut bin = r.shown.bin;
                        if ui
                            .add_sized(
                                [widths[6], ROW_H - 2.0],
                                egui::DragValue::new(&mut bin).range(0..=4),
                            )
                            .changed()
                        {
                            props.bin = Some(bin);
                            changed = true;
                        }
                        let mut retries = r.shown.pick_retries;
                        if ui
                            .add_sized(
                                [widths[7], ROW_H - 2.0],
                                egui::DragValue::new(&mut retries).range(0..=10),
                            )
                            .changed()
                        {
                            props.pick_retries = Some(retries);
                            changed = true;
                        }
                        if r.defined && ui.small_button("x").clicked() {
                            remove = Some(r.name.clone());
                        }
                    });
                    if changed {
                        pending = Some((r.name.clone(), props));
                    }
                }
            });
        if let Some((name, props)) = pending {
            self.s.edit_package(&name, props);
        }
        if let Some(name) = remove {
            self.s.delete_package(&name);
        }
        ui.separator();
        ui.label(
            RichText::new(tr(Text::KindsHeading, lang))
                .strong()
                .color(col(pal.accent)),
        );
        let kinds = kind_rows(&self.s.ws);
        let mut kw = self
            .s
            .ui
            .column_widths("kinds", &[110.0, 140.0, 60.0, 80.0, 50.0, 80.0, 80.0, 40.0]);
        if let Some(i) = header(
            ui,
            &pal,
            "kinds_head",
            &mut kw,
            &[
                tr(Text::ColValue, lang),
                tr(Text::ColPackage, lang),
                tr(Text::ColKinds, lang),
                tr(Text::ColHeight, lang),
                tr(Text::ColRank, lang),
                tr(Text::ColTip, lang),
                tr(Text::ColTape, lang),
                "",
            ],
        ) {
            self.s.ui.set_column_width("kinds", i, kw[i]);
        }
        egui::ScrollArea::vertical()
            .id_salt("kind_rows")
            .auto_shrink([false, false])
            .show_rows(ui, ROW_H, kinds.len(), |ui, range| {
                for i in range {
                    let r = &kinds[i];
                    ui.horizontal(|ui| {
                        cell(ui, kw[0], r.value.clone());
                        cell(ui, kw[1], r.package.clone());
                        cell(ui, kw[2], r.count.to_string());
                        cell(ui, kw[3], format!("{:.2}", r.height_mm));
                        cell(ui, kw[4], r.rank.to_string());
                        cell(ui, kw[5], r.tip.clone());
                        cell(ui, kw[6], format!("{:.1}", r.tape_mm));
                        if !r.described {
                            cell(ui, kw[7], RichText::new("!").color(col(pal.warn)));
                        }
                    });
                }
            });
    }

    fn feeders_tab(&mut self, ui: &mut egui::Ui) {
        let pal = self.s.ui.theme.palette();
        let lang = self.s.lang();
        let mut rows = feeder_rows(&self.s.ws, self.s.session.job());
        let mut sort = self.s.feeder_sort;
        ui.horizontal(|ui| {
            ui.label(format!("{} {}", rows.len(), tr(Text::TabFeeders, lang)));
            sort_picker(
                ui,
                "feeder_sort",
                &[
                    (FeederColumn::Id, Text::ColId),
                    (FeederColumn::Value, Text::ColValue),
                    (FeederColumn::Package, Text::ColPackage),
                    (FeederColumn::Count, Text::ColCount),
                    (FeederColumn::Left, Text::Left),
                    (FeederColumn::Slot, Text::ColSlot),
                ],
                &mut sort,
                lang,
            );
            if ui
                .add_enabled(
                    self.s.ws.can_undo(),
                    egui::Button::new(tr(Text::Undo, lang)),
                )
                .clicked()
            {
                self.s.undo();
            }
            if ui.button(tr(Text::AddFeeder, lang)).clicked() {
                self.s.add_feeder();
            }
        });
        self.s.feeder_sort = sort;
        sort_feeders(&mut rows, sort);
        self.feeder_form(ui);
        let mut widths = self.s.ui.column_widths(
            "feeders",
            &[
                60.0, 110.0, 130.0, 60.0, 60.0, 80.0, 80.0, 70.0, 60.0, 70.0, 40.0,
            ],
        );
        if let Some(i) = header(
            ui,
            &pal,
            "feeders_head",
            &mut widths,
            &[
                tr(Text::ColId, lang),
                tr(Text::ColValue, lang),
                tr(Text::ColPackage, lang),
                tr(Text::ColX, lang),
                tr(Text::ColY, lang),
                tr(Text::ColCount, lang),
                tr(Text::Left, lang),
                tr(Text::ColSlot, lang),
                tr(Text::ColKind, lang),
                tr(Text::ColPriority, lang),
                tr(Text::ColEnabled, lang),
            ],
        ) {
            self.s.ui.set_column_width("feeders", i, widths[i]);
        }
        let mut commit = false;
        let mut cancel = false;
        egui::ScrollArea::vertical()
            .id_salt("feeder_rows")
            .auto_shrink([false, false])
            .show_rows(ui, ROW_H, rows.len(), |ui, range| {
                for i in range {
                    let r = &rows[i];
                    let chosen = self.s.selection.is_feeder(&r.id);
                    let editing = self.s.edit.feeder.as_deref() == Some(r.id.as_str());
                    let mut count_rect = Rect::NOTHING;
                    let resp = row_ui(
                        ui,
                        egui::Id::unique("feeder_row").with(i),
                        chosen,
                        &pal,
                        !editing,
                        |ui| {
                            cell(ui, widths[0], r.id.clone());
                            cell(ui, widths[1], r.value.clone());
                            cell(ui, widths[2], r.package.clone());
                            cell(ui, widths[3], format!("{:.1}", r.x));
                            cell(ui, widths[4], format!("{:.1}", r.y));
                            if editing {
                                let edit = ui.add_sized(
                                    [widths[5], ROW_H - 2.0],
                                    egui::TextEdit::singleline(&mut self.s.edit.text),
                                );
                                edit.request_focus();
                                let enter = ui.input(|inp| inp.key_pressed(egui::Key::Enter));
                                let escape = ui.input(|inp| inp.key_pressed(egui::Key::Escape));
                                if escape {
                                    cancel = true;
                                } else if enter || edit.lost_focus() {
                                    commit = true;
                                }
                            } else {
                                let c = ui.add_sized(
                                    [widths[5], ROW_H - 2.0],
                                    egui::Label::new(r.count.to_string()).truncate(),
                                );
                                count_rect = c.rect;
                            }
                            let left = if r.left < r.count {
                                RichText::new(r.left.to_string()).color(col(pal.warn))
                            } else {
                                RichText::new(r.left.to_string())
                            };
                            cell(ui, widths[6], left);
                            cell(ui, widths[7], r.slot.clone());
                            let kind = match r.kind {
                                "tray" => Text::KindTray,
                                "strip" => Text::KindStrip,
                                _ => Text::KindTape,
                            };
                            cell(ui, widths[8], tr(kind, lang));
                            let prio = match r.priority {
                                p if p < 0 => Text::PrioLow,
                                0 => Text::PrioNormal,
                                _ => Text::PrioHigh,
                            };
                            cell(ui, widths[9], tr(prio, lang));
                            cell(ui, widths[10], if r.enabled { "+" } else { "-" });
                        },
                    );
                    if resp.clicked() {
                        let on_count = resp
                            .interact_pointer_pos()
                            .is_some_and(|pos| count_rect.contains(pos));
                        if on_count {
                            self.s
                                .edit
                                .click_count(&mut self.s.selection, &r.id, r.count);
                        } else {
                            self.s.selection.select_feeder(&r.id);
                            self.s.edit.cancel();
                        }
                    }
                }
            });
        if cancel {
            self.s.edit.cancel();
        } else if commit {
            if let Some((id, n)) = self.s.edit.finish() {
                self.s.set_feeder_count(&id, n);
            } else {
                self.s.edit.cancel();
            }
        }
    }

    /// The form of the feeder that is chosen: everything about it that the table does not show, and the ways to change it.
    fn feeder_form(&mut self, ui: &mut egui::Ui) {
        let pal = self.s.ui.theme.palette();
        let lang = self.s.lang();
        let Some(id) = self.s.selection.feeder.clone() else {
            return;
        };
        let Some(index) = self.s.ws.feeders.iter().position(|f| f.base.id == id) else {
            return;
        };
        let mut spec = self.s.ws.feeders[index].clone();
        let before = spec.clone();
        let mut delete = false;
        ui.group(|ui| {
            ui.label(
                RichText::new(format!("{}: {id}", tr(Text::SelectedFeeder, lang)))
                    .strong()
                    .color(col(pal.accent)),
            );
            ui.horizontal_wrapped(|ui| {
                ui.label(tr(Text::ColValue, lang));
                ui.add(egui::TextEdit::singleline(&mut spec.base.value).desired_width(90.0));
                ui.label(tr(Text::ColPackage, lang));
                ui.add(egui::TextEdit::singleline(&mut spec.base.package).desired_width(90.0));
                ui.label(tr(Text::ColX, lang));
                ui.add(
                    egui::DragValue::new(&mut spec.base.x)
                        .speed(0.1)
                        .max_decimals(3),
                );
                ui.label(tr(Text::ColY, lang));
                ui.add(
                    egui::DragValue::new(&mut spec.base.y)
                        .speed(0.1)
                        .max_decimals(3),
                );
                ui.label(tr(Text::ColCount, lang));
                ui.add(egui::DragValue::new(&mut spec.base.count).range(0..=1_000_000));
                ui.label(tr(Text::ColWarn, lang));
                ui.add(egui::DragValue::new(&mut spec.warn_below).range(0..=1_000_000));
                ui.label(tr(Text::ColSlot, lang));
                let mut slot = spec.slot.clone().unwrap_or_default();
                if ui
                    .add(egui::TextEdit::singleline(&mut slot).desired_width(60.0))
                    .changed()
                {
                    let slot = slot.trim();
                    spec.slot = (!slot.is_empty()).then(|| slot.chars().take(32).collect());
                }
                ui.checkbox(&mut spec.enabled, tr(Text::ColEnabled, lang));
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(tr(Text::ColPriority, lang));
                let prio_text = match spec.priority {
                    p if p < 0 => tr(Text::PrioLow, lang),
                    0 => tr(Text::PrioNormal, lang),
                    _ => tr(Text::PrioHigh, lang),
                };
                egui::ComboBox::from_id_salt("feeder_priority")
                    .selected_text(prio_text)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut spec.priority, -1, tr(Text::PrioLow, lang));
                        ui.selectable_value(&mut spec.priority, 0, tr(Text::PrioNormal, lang));
                        ui.selectable_value(&mut spec.priority, 1, tr(Text::PrioHigh, lang));
                    });
                ui.label(tr(Text::ColKind, lang));
                let mut kind = match spec.kind {
                    FeederKind::Tape => 0,
                    FeederKind::Tray { .. } => 1,
                    FeederKind::Strip { .. } => 2,
                };
                let was = kind;
                let kind_text = [
                    tr(Text::KindTape, lang),
                    tr(Text::KindTray, lang),
                    tr(Text::KindStrip, lang),
                ];
                egui::ComboBox::from_id_salt("feeder_kind")
                    .selected_text(kind_text[kind])
                    .show_ui(ui, |ui| {
                        for (n, t) in kind_text.iter().enumerate() {
                            ui.selectable_value(&mut kind, n, *t);
                        }
                    });
                if kind != was {
                    spec.kind = match kind {
                        1 => FeederKind::Tray {
                            cols: 4,
                            rows: 4,
                            pitch_x: 10.0,
                            pitch_y: 10.0,
                            rotation: 0.0,
                        },
                        2 => FeederKind::Strip {
                            pitch: 4.0,
                            rotation: 0.0,
                        },
                        _ => FeederKind::Tape,
                    };
                }
                match &mut spec.kind {
                    FeederKind::Tape => {
                        ui.label(tr(Text::ColRot, lang));
                        ui.add(
                            egui::DragValue::new(&mut spec.tape_rotation)
                                .range(0.0..=359.0)
                                .speed(1.0),
                        );
                    }
                    FeederKind::Tray {
                        cols,
                        rows,
                        pitch_x,
                        pitch_y,
                        rotation,
                    } => {
                        ui.label(tr(Text::ColCols, lang));
                        ui.add(egui::DragValue::new(cols).range(1..=1000));
                        ui.label(tr(Text::ColRows, lang));
                        ui.add(egui::DragValue::new(rows).range(1..=1000));
                        ui.label(tr(Text::ColPitchX, lang));
                        ui.add(egui::DragValue::new(pitch_x).speed(0.1).max_decimals(3));
                        ui.label(tr(Text::ColPitchY, lang));
                        ui.add(egui::DragValue::new(pitch_y).speed(0.1).max_decimals(3));
                        ui.label(tr(Text::ColRot, lang));
                        ui.add(egui::DragValue::new(rotation).range(0.0..=359.0));
                        ui.label(tr(Text::ColFill, lang));
                        let fills = [
                            (TrayFill::Rows, Text::FillRows),
                            (TrayFill::Columns, Text::FillColumns),
                            (TrayFill::Snake, Text::FillSnake),
                        ];
                        let current = fills
                            .iter()
                            .find(|(f, _)| *f == spec.fill)
                            .map_or("", |(_, t)| tr(*t, lang));
                        egui::ComboBox::from_id_salt("feeder_fill")
                            .selected_text(current)
                            .show_ui(ui, |ui| {
                                for (f, t) in fills {
                                    ui.selectable_value(&mut spec.fill, f, tr(t, lang));
                                }
                            });
                    }
                    FeederKind::Strip { pitch, rotation } => {
                        ui.label(tr(Text::ColPitch, lang));
                        ui.add(egui::DragValue::new(pitch).speed(0.1).max_decimals(3));
                        ui.label(tr(Text::ColRot, lang));
                        ui.add(egui::DragValue::new(rotation).range(0.0..=359.0));
                    }
                }
                if ui.button(tr(Text::DeleteRow, lang)).clicked() {
                    delete = true;
                }
            });
        });
        if delete {
            self.s.delete_feeder(index);
        } else if spec != before {
            self.s.edit_feeder(index, spec);
        }
    }

    /// The panels: their children with the places, and the fiducials with the fit that moves the parts to where the panel really is.
    fn panels_tab(&mut self, ui: &mut egui::Ui) {
        let pal = self.s.ui.theme.palette();
        let lang = self.s.lang();
        let Some(panel) = self.s.ws.panel.clone() else {
            ui.label(RichText::new(tr(Text::NoPanel, lang)).color(col(pal.dim)));
            return;
        };
        ui.label(
            RichText::new(tr(Text::PanelChildren, lang))
                .strong()
                .color(col(pal.accent)),
        );
        let children = panel.children();
        let mut widths = self
            .s
            .ui
            .column_widths("children", &[90.0, 70.0, 70.0, 100.0, 70.0, 70.0, 60.0]);
        if let Some(i) = header(
            ui,
            &pal,
            "children_head",
            &mut widths,
            &[
                tr(Text::ColPanel, lang),
                tr(Text::ColChild, lang),
                tr(Text::ColKind, lang),
                tr(Text::ColName, lang),
                tr(Text::ColX, lang),
                tr(Text::ColY, lang),
                tr(Text::ColRot, lang),
            ],
        ) {
            self.s.ui.set_column_width("children", i, widths[i]);
        }
        egui::ScrollArea::vertical()
            .id_salt("child_rows")
            .max_height(180.0)
            .show_rows(ui, ROW_H, children.len(), |ui, range| {
                for i in range {
                    let c = &children[i];
                    ui.horizontal(|ui| {
                        let root = c.panel == panel.root();
                        let name = if root {
                            RichText::new(format!("{} *", c.panel))
                        } else {
                            RichText::new(c.panel.clone())
                        };
                        cell(ui, widths[0], name);
                        cell(ui, widths[1], c.id.clone());
                        cell(
                            ui,
                            widths[2],
                            if c.is_board {
                                tr(Text::SrcBoard, lang)
                            } else {
                                tr(Text::KindPanel, lang)
                            },
                        );
                        cell(ui, widths[3], c.target.clone());
                        cell(ui, widths[4], format!("{:.3}", c.x));
                        cell(ui, widths[5], format!("{:.3}", c.y));
                        cell(ui, widths[6], format!("{:.1}", c.rotation));
                    });
                }
            });
        ui.separator();
        ui.label(
            RichText::new(tr(Text::Fiducials, lang))
                .strong()
                .color(col(pal.accent)),
        );
        if self.s.fid_edit.len() != self.s.ws.design_fiducials.len() {
            self.s.fid_edit = fid_buffer(&self.s.ws);
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr(Text::LoadMarks, lang)).clicked() {
                self.s.open_browser(Purpose::Fiducials);
            }
            let any_on = self.s.fid_edit.iter().any(|e| e.on);
            if ui
                .add_enabled(any_on, egui::Button::new(tr(Text::ApplyFit, lang)))
                .clicked()
            {
                self.s.apply_fiducials();
            }
            let has = !self.s.ws.measured.is_empty();
            if ui
                .add_enabled(has, egui::Button::new(tr(Text::ClearFit, lang)))
                .clicked()
            {
                self.s.clear_fiducials();
            }
        });
        if let Some(f) = &self.s.ws.fit {
            ui.label(format!(
                "{}: {}, {} - {} {:.3} mm - {} {:.3} mm - {} {:.3}",
                tr(Text::FitSummary, lang),
                f.mode.name(),
                f.ids.len(),
                tr(Text::ResidualRms, lang),
                f.rms,
                tr(Text::ResidualMax, lang),
                f.max,
                tr(Text::FitTurn, lang),
                f.transform.rotation_deg()
            ));
        } else if let Some(e) = &self.s.ws.fit_error {
            ui.label(RichText::new(e.as_str()).color(col(pal.danger)));
        } else {
            ui.label(RichText::new(tr(Text::FitNone, lang)).color(col(pal.dim)));
        }
        let rows = fiducial_rows(&self.s.ws);
        let mut fw = self
            .s
            .ui
            .column_widths("fids", &[70.0, 80.0, 80.0, 40.0, 110.0, 110.0, 90.0]);
        if let Some(i) = header(
            ui,
            &pal,
            "fids_head",
            &mut fw,
            &[
                tr(Text::ColId, lang),
                tr(Text::ColX, lang),
                tr(Text::ColY, lang),
                tr(Text::ColUse, lang),
                tr(Text::ColMeasuredX, lang),
                tr(Text::ColMeasuredY, lang),
                tr(Text::ColResidual, lang),
            ],
        ) {
            self.s.ui.set_column_width("fids", i, fw[i]);
        }
        let mut edits = self.s.fid_edit.clone();
        egui::ScrollArea::vertical()
            .id_salt("fiducial_rows")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (r, e) in rows.iter().zip(edits.iter_mut()) {
                    ui.horizontal(|ui| {
                        cell(ui, fw[0], r.id.clone());
                        cell(ui, fw[1], format!("{:.3}", r.x));
                        cell(ui, fw[2], format!("{:.3}", r.y));
                        ui.add_sized(
                            [fw[3], ROW_H - 2.0],
                            egui::Checkbox::without_text(&mut e.on),
                        );
                        ui.add_sized(
                            [fw[4], ROW_H - 2.0],
                            egui::DragValue::new(&mut e.x).speed(0.05).max_decimals(3),
                        );
                        ui.add_sized(
                            [fw[5], ROW_H - 2.0],
                            egui::DragValue::new(&mut e.y).speed(0.05).max_decimals(3),
                        );
                        let residual = r.residual.map_or(String::new(), |v| format!("{v:.3}"));
                        cell(ui, fw[6], residual);
                    });
                }
            });
        if edits != self.s.fid_edit {
            self.s.fid_edit = edits;
        }
    }

    /// The boards of the panel and how many times it places each one.
    fn boards_tab(&mut self, ui: &mut egui::Ui) {
        let pal = self.s.ui.theme.palette();
        let lang = self.s.lang();
        let rows = board_rows(&self.s.ws);
        if self.s.ws.panel.is_none() {
            ui.label(RichText::new(tr(Text::SingleBoard, lang)).color(col(pal.dim)));
            let name = self
                .s
                .ws
                .names
                .get(&FileKind::Parts)
                .cloned()
                .unwrap_or_default();
            ui.label(format!(
                "{name}: {} {}",
                self.s.ws.parts.len(),
                tr(Text::SumParts, lang)
            ));
            return;
        }
        ui.label(
            RichText::new(format!(
                "{} - {} {}",
                tr(Text::BoardsOfPanel, lang),
                self.s.ws.parts.len(),
                tr(Text::SumParts, lang)
            ))
            .strong()
            .color(col(pal.accent)),
        );
        let mut widths = self.s.ui.column_widths("boards", &[110.0, 260.0, 70.0]);
        if let Some(i) = header(
            ui,
            &pal,
            "boards_head",
            &mut widths,
            &[
                tr(Text::ColId, lang),
                tr(Text::ColFile, lang),
                tr(Text::ColCopies, lang),
            ],
        ) {
            self.s.ui.set_column_width("boards", i, widths[i]);
        }
        egui::ScrollArea::vertical()
            .id_salt("board_rows")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for r in &rows {
                    ui.horizontal(|ui| {
                        cell(ui, widths[0], r.id.clone());
                        cell(ui, widths[1], r.file.clone());
                        cell(ui, widths[2], r.copies.to_string());
                    });
                }
            });
    }

    /// The window that picks a file or a folder.
    fn browser_window(&mut self, ctx: &egui::Context) {
        let Some(mut b) = self.s.browser.take() else {
            return;
        };
        let pal = self.s.ui.theme.palette();
        let lang = self.s.lang();
        let title = match b.purpose {
            Purpose::Files => tr(Text::PickFiles, lang),
            Purpose::Fiducials => tr(Text::PickMarks, lang),
            Purpose::Folder => tr(Text::PickFolder, lang),
        };
        let mut picked: Option<PathBuf> = None;
        let mut use_folder = false;
        let mut close = false;
        let mut open = true;
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .default_size([620.0, 460.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(tr(Text::UpFolder, lang)).clicked() {
                        b.up();
                    }
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut b.path_text)
                            .desired_width(ui.available_width()),
                    );
                    if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if let Some(p) = b.go_typed() {
                            picked = Some(p);
                        }
                    }
                });
                if let Some(e) = &b.error {
                    ui.label(RichText::new(e.as_str()).color(col(pal.danger)));
                }
                if b.purpose != Purpose::Folder
                    && ui
                        .checkbox(&mut b.show_all, tr(Text::AllFiles, lang))
                        .changed()
                {
                    b.refresh();
                }
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        if b.entries.is_empty() {
                            ui.label(
                                RichText::new(tr(Text::EmptyFolder, lang)).color(col(pal.dim)),
                            );
                        }
                        for e in b.entries.clone() {
                            let label = if e.is_dir {
                                format!("{}/", e.name)
                            } else {
                                format!("{}    ({})", e.name, browse::size_text(e.size))
                            };
                            if ui.selectable_label(false, label).clicked() {
                                if let Some(p) = b.enter(&e.name) {
                                    picked = Some(p);
                                }
                            }
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    if b.purpose == Purpose::Folder
                        && ui.button(tr(Text::UseFolder, lang)).clicked()
                    {
                        use_folder = true;
                    }
                    if ui.button(tr(Text::Close, lang)).clicked() {
                        close = true;
                    }
                });
            });
        self.s.browser = Some(b);
        if let Some(p) = picked {
            self.s.browser_pick(p);
        }
        if use_folder {
            self.s.browser_use_folder();
        } else if close || !open {
            self.s.close_browser();
        }
    }

    fn load_sheet_tab(&mut self, ui: &mut egui::Ui) {
        let pal = self.s.ui.theme.palette();
        let lang = self.s.lang();
        let Some(sheet) = self.s.ws.load_sheet() else {
            ui.label(RichText::new(tr(Text::SlotsMissing, lang)).color(col(pal.dim)));
            return;
        };
        let mut widths = self
            .s
            .ui
            .column_widths("sheet", &[60.0, 110.0, 130.0, 60.0, 80.0, 80.0, 60.0, 60.0]);
        if let Some(i) = header(
            ui,
            &pal,
            "sheet_head",
            &mut widths,
            &[
                tr(Text::ColSlot, lang),
                tr(Text::ColValue, lang),
                tr(Text::ColPackage, lang),
                tr(Text::ColCount, lang),
                tr(Text::ColTape, lang),
                tr(Text::ColTip, lang),
                tr(Text::ColX, lang),
                tr(Text::ColY, lang),
            ],
        ) {
            self.s.ui.set_column_width("sheet", i, widths[i]);
        }
        egui::ScrollArea::vertical()
            .id_salt("sheet_rows")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for r in &sheet.rows {
                    ui.horizontal(|ui| {
                        cell(ui, widths[0], r.slot.id.clone());
                        cell(ui, widths[1], r.need.value.clone());
                        cell(ui, widths[2], r.need.package.clone());
                        cell(ui, widths[3], r.need.count.to_string());
                        cell(ui, widths[4], format!("{:.1}", r.need.tape_mm));
                        cell(ui, widths[5], r.need.tip.clone());
                        cell(ui, widths[6], format!("{:.1}", r.slot.x));
                        cell(ui, widths[7], format!("{:.1}", r.slot.y));
                    });
                }
                if !sheet.unassigned.is_empty() {
                    ui.separator();
                    ui.label(
                        RichText::new(tr(Text::Unassigned, lang))
                            .strong()
                            .color(col(pal.danger)),
                    );
                    for n in &sheet.unassigned {
                        ui.label(format!("{} / {} ({})", n.value, n.package, n.count));
                    }
                }
            });
    }

    fn machine_tab(&mut self, ui: &mut egui::Ui) {
        let pal = self.s.ui.theme.palette();
        let lang = self.s.lang();
        let mut nozzles = self.s.ws.cfg.nozzles;
        let mut speed = self.s.ws.cfg.x_speed_mm_s;
        let mut order = self.s.ws.cfg.order;
        let mut side = self.s.ws.options.side;
        let mut rate = self.s.session.rate();
        let (mut machine_changed, mut order_changed, mut side_changed, mut rate_changed) =
            (false, false, false, false);
        egui::Grid::new("machine_grid")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label(tr(Text::Side, lang));
                egui::ComboBox::from_id_salt("side")
                    .selected_text(if side == Side::Top {
                        tr(Text::SideTop, lang)
                    } else {
                        tr(Text::SideBottom, lang)
                    })
                    .show_ui(ui, |ui| {
                        side_changed |= ui
                            .selectable_value(&mut side, Side::Top, tr(Text::SideTop, lang))
                            .changed();
                        side_changed |= ui
                            .selectable_value(&mut side, Side::Bottom, tr(Text::SideBottom, lang))
                            .changed();
                    });
                ui.end_row();
                ui.label(tr(Text::Nozzles, lang));
                machine_changed |= ui
                    .add(egui::DragValue::new(&mut nozzles).range(1..=6))
                    .changed();
                ui.end_row();
                ui.label(tr(Text::Speed, lang));
                machine_changed |= ui
                    .add(
                        egui::DragValue::new(&mut speed)
                            .range(1.0..=5000.0)
                            .speed(5.0),
                    )
                    .changed();
                ui.end_row();
                ui.label(tr(Text::Order, lang));
                egui::ComboBox::from_id_salt("order")
                    .selected_text(order.name())
                    .show_ui(ui, |ui| {
                        for o in ORDERS {
                            order_changed |= ui.selectable_value(&mut order, o, o.name()).changed();
                        }
                    });
                ui.end_row();
                ui.label("gui_steps_per_s");
                rate_changed |= ui
                    .add(
                        egui::DragValue::new(&mut rate)
                            .range(0.1..=1000.0)
                            .speed(0.1),
                    )
                    .changed();
                ui.end_row();
            });
        if side_changed {
            self.s.set_side(side);
        }
        if machine_changed {
            self.s.set_machine(nozzles, speed);
        }
        if order_changed {
            self.s.set_order(order);
        }
        if rate_changed {
            self.s.ws.cfg.gui_steps_per_s = rate;
            self.s.session.set_rate(rate);
        }
        ui.separator();
        let cfg = &self.s.ws.cfg;
        let facts = [
            ("pick_s", cfg.pick_s.to_string()),
            ("place_s", cfg.place_s.to_string()),
            ("feed_s", cfg.feed_s.to_string()),
            ("tip_change_s", cfg.tip_change_s.to_string()),
            ("x_accel_mm_s2", cfg.x_accel_mm_s2.to_string()),
            ("y_accel_mm_s2", cfg.y_accel_mm_s2.to_string()),
            ("error_policy", format!("{:?}", cfg.error_policy)),
            ("fault_limit", cfg.fault_limit.to_string()),
            ("fault_action", cfg.fault_action.name().to_string()),
            (
                "vacuum",
                if cfg.vacuum { "on" } else { "off" }.to_string(),
            ),
            ("sim_seed", cfg.sim_seed.to_string()),
            ("gui_theme", cfg.gui_theme.clone()),
        ];
        egui::Grid::new("machine_facts")
            .num_columns(2)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                for (k, v) in &facts {
                    ui.label(RichText::new(*k).color(col(pal.dim)));
                    ui.label(v);
                    ui.end_row();
                }
            });
    }

    fn problems_tab(&mut self, ui: &mut egui::Ui) {
        let pal = self.s.ui.theme.palette();
        let lang = self.s.lang();
        if self.s.problems.is_empty() {
            ui.label(RichText::new(tr(Text::NoProblems, lang)).color(col(pal.ok)));
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.s.ws.can_undo(),
                    egui::Button::new(tr(Text::Undo, lang)),
                )
                .clicked()
            {
                self.s.undo();
            }
        });
        let mut apply: Option<Fix> = None;
        egui::ScrollArea::vertical()
            .id_salt("problem_rows")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for p in &self.s.problems {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(p.severity_text(lang))
                                .strong()
                                .color(severity_color(p.severity, &pal)),
                        );
                        ui.label(RichText::new(p.about(lang)).color(col(pal.accent)));
                        ui.label(p.line(lang));
                        if let (Some(fix), Some(text)) = (&p.fix, p.fix_text(lang)) {
                            if ui
                                .button(format!("{} - {text}", tr(Text::Apply, lang)))
                                .clicked()
                            {
                                apply = Some(fix.clone());
                            }
                        }
                    });
                }
            });
        if let Some(fix) = apply {
            self.s.apply(&fix);
        }
    }

    fn log_tab(&mut self, ui: &mut egui::Ui) {
        let pal = self.s.ui.theme.palette();
        let lang = self.s.lang();
        ui.horizontal(|ui| {
            if ui.button(tr(Text::ClearLog, lang)).clicked() {
                self.s.log.clear();
            }
            if ui.button(tr(Text::CopyDiagnostics, lang)).clicked() {
                let text = diagnostics(&self.s.ws, &self.s.log);
                ui.ctx().copy_text(text);
            }
        });
        egui::ScrollArea::vertical()
            .id_salt("log_rows")
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for e in self.s.log.entries() {
                    let c = match e.level {
                        super::log::Level::Info => col(pal.dim),
                        super::log::Level::Warn => col(pal.warn),
                        super::log::Level::Error => col(pal.danger),
                    };
                    ui.label(RichText::new(&e.text).monospace().color(c));
                }
            });
    }
}

impl eframe::App for PnpApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let theme = self.s.ui.theme;
        if self.visuals_for != Some(theme) {
            apply_visuals(&ctx, &theme.palette());
            self.visuals_for = Some(theme);
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        if !dropped.is_empty() {
            self.s.load_paths(&dropped);
        }
        if self.s.session.is_running() {
            let dt = f64::from(ctx.input(|i| i.stable_dt));
            self.s.tick(dt);
            ctx.request_repaint();
        }
        self.top_bar(ui);
        self.path_bar(ui);
        self.status_bar(ui);
        self.side_panel(ui);
        self.center(ui);
        self.browser_window(&ctx);
        self.error_window(&ctx);
        // an edit of a table ends when nothing is being dragged or typed in
        let busy = ctx.input(|i| i.pointer.any_down()) || ctx.memory(|m| m.focused().is_some());
        if !busy {
            self.s.end_edit();
        }
        // in the picture mode nothing of the person is written
        if self.shot.is_none() && self.s.ui != self.saved {
            settings::save(&self.s.ui);
            self.saved = self.s.ui.clone();
        }
        self.shot_tick(&ctx);
    }
}

/// Opens the window; the files named are loaded at once.
pub fn run(paths: &[PathBuf]) -> eframe::Result {
    let ui_settings = settings::load();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(Vec2::new(1280.0, 800.0))
        .with_min_inner_size([900.0, 560.0])
        .with_title("EH3D-PHOENIX-PNP");
    if let Some(icon) = icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    let paths = paths.to_vec();
    eframe::run_native(
        "EH3D-PHOENIX-PNP",
        options,
        Box::new(move |cc| {
            install_fonts(&cc.egui_ctx);
            Ok(Box::new(PnpApp::new(ui_settings, &paths)))
        }),
    )
}

/// How the picture mode ended: nothing yet, the file was written, or the reason it was not.
type ShotOutcome = Arc<Mutex<Option<Result<(), String>>>>;

/// The picture being taken.
struct ShotState {
    request: ShotRequest,
    frame: u32,
    outcome: ShotOutcome,
}

impl PnpApp {
    /// Counts the frames, asks for the picture after the frames waited, writes it and closes the window when it is written or when it does not come.
    fn shot_tick(&mut self, ctx: &egui::Context) {
        let Some(shot) = self.shot.as_mut() else {
            return;
        };
        shot.frame += 1;
        ctx.request_repaint();
        let due = 3 + shot.request.wait;
        let finished = match shot.outcome.lock() {
            Ok(slot) => slot.is_some(),
            Err(_) => true,
        };
        if finished {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if shot.frame == due {
            let (output, replace) = (shot.request.output.clone(), shot.request.replace);
            let (outcome, repaint) = (shot.outcome.clone(), ctx.clone());
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                egui::ScreenshotCallback::new(move |image| {
                    let result = save_shot_png(&image, &output, replace);
                    if let Ok(mut slot) = outcome.lock() {
                        *slot = Some(result);
                    }
                    repaint.request_repaint();
                }),
            ));
        } else if shot.frame > due + PATIENCE_FRAMES {
            if let Ok(mut slot) = shot.outcome.lock() {
                *slot = Some(Err(
                    "the picture did not come back from the window".to_owned()
                ));
            }
        }
    }
}

/// Writes the picture the window gave back as a PNG file, a new file unless `replace` is true.
fn save_shot_png(image: &egui::ColorImage, path: &Path, replace: bool) -> Result<(), String> {
    let mut open = std::fs::OpenOptions::new();
    open.write(true);
    if replace {
        open.create(true).truncate(true);
    } else {
        open.create_new(true);
    }
    let file = open
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(file),
        image.size[0] as u32,
        image.size[1] as u32,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    let data: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
    writer.write_image_data(&data).map_err(|e| e.to_string())?;
    // the end of the file is written here, so that a failure is reported and not lost in a drop
    writer.finish().map_err(|e| e.to_string())
}

/// The most files the picture mode loads.
const SHOT_MAX_FILES: usize = 64;

/// Opens the window at the size asked with the files loaded, in the language and the theme asked, saves the picture as the request says and closes by itself. The window reads and
/// writes no settings (the guide is not shown). The answer is the reason when the picture could not be written.
pub fn run_shot(
    files: &[PathBuf],
    lang: Lang,
    request: ShotRequest,
    view: (Option<Tab>, u32),
) -> Result<(), String> {
    if !request.replace && request.output.exists() {
        return Err(format!(
            "{} exists; give --replace to write over it",
            request.output.display()
        ));
    }
    let folder = request
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf);
    if folder.is_some_and(|dir| !dir.is_dir()) {
        return Err(format!(
            "the folder of {} does not exist",
            request.output.display()
        ));
    }
    if let Some(missing) = files.iter().find(|f| !f.is_file()) {
        return Err(format!("{} is not a file", missing.display()));
    }
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(Vec2::new(request.size.0 as f32, request.size.1 as f32))
        .with_title("EH3D-PHOENIX-PNP");
    if let Some(icon) = icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    let ui_settings = UiSettings {
        lang,
        theme: request.theme,
        guide_seen: true,
        ..UiSettings::default()
    };
    let paths = files.to_vec();
    let outcome: ShotOutcome = Arc::new(Mutex::new(None));
    let outcome_for_app = outcome.clone();
    eframe::run_native(
        "EH3D-PHOENIX-PNP",
        options,
        Box::new(move |cc| {
            install_fonts(&cc.egui_ctx);
            let mut app = PnpApp::new(ui_settings, &paths);
            // the language that was asked for wins over the one that a configuration file loaded says
            app.s.set_lang(lang);
            if let Some(tab) = view.0 {
                app.s.tab = tab;
            }
            if view.1 > 0 {
                app.s.run_steps(view.1);
            }
            app.shot = Some(ShotState {
                request,
                frame: 0,
                outcome: outcome_for_app,
            });
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| e.to_string())?;
    let taken = outcome.lock().map_err(|e| e.to_string())?.take();
    match taken {
        Some(result) => result,
        None => Err("the window closed before the picture was taken".to_owned()),
    }
}

/// The picture mode as the program runs it: reads the arguments (`--shot FILE.png` and the options of [`shot::parse_window_args`]), takes the picture and gives the exit code: 0 when
/// the file was written, 1 when it was not and 2 when the arguments are wrong. Nothing is written beside the picture.
pub fn take_picture(args: &[String]) -> u8 {
    let parsed = match shot::parse_window_args(args, SHOT_MAX_FILES) {
        Ok(parsed) => parsed,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let Some(request) = parsed.shot else {
        eprintln!("error: --shot needs a file to write");
        return 2;
    };
    let view = (parsed.window, parsed.steps.unwrap_or(0));
    match run_shot(
        &parsed.files,
        parsed.lang.unwrap_or(Lang::En),
        request,
        view,
    ) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: the picture could not be taken: {e}");
            1
        }
    }
}
