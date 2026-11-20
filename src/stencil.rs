// EH3D-PHOENIX-PNP - the openings of the solder paste stencil, from the pads of the footprints.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The paste is printed through a stencil whose opening for a pad is a little smaller than the pad (a reduction of each side by a few per cent, more for the pads of a fine pitch), and
//! whose opening for the big thermal pad of a part is split into windows that cover only part of it, so that the part does not float. This makes the openings from the pads of a table of
//! features (`dfm::Feature`, the rows of kind `pad`) and checks them: the thickness of the stencil, the reductions and the coverage of the thermal windows against their ranges, the web
//! between windows, and the area ratio (the area of the opening over the area of its walls, `L*W / (2*(L+W)*T)`) that decides whether the paste leaves the walls. Every figure is a setting
//! (`stencil_*`), the ranges are the public ones of the guides and none has been compared with the rules of a stencil maker. It reads and says; it changes nothing.

use crate::config::MachineConfig;
use crate::dfm::{Feature, Kind};
use crate::error::Lang;
use crate::parts::csv_field;

/// The most openings that are made.
pub const MAX_APERTURES: usize = 500_000;

/// The settings of the stencil (the keys `stencil_*`).
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub thickness: f64,
    pub thickness_min: f64,
    pub thickness_max: f64,
    /// The reduction of each side of an opening, per cent, and the range it should be in.
    pub reduction: f64,
    pub reduction_min: f64,
    pub reduction_max: f64,
    /// The same for the pads of a fine pitch, and the pitch (mm) at or under which a pad is of a fine pitch.
    pub fine_reduction: f64,
    pub fine_min: f64,
    pub fine_max: f64,
    pub fine_pitch: f64,
    pub area_ratio_min: f64,
    /// The share (per cent) of a thermal pad that its windows cover and the range it should be in.
    pub thermal_percent: f64,
    pub thermal_min: f64,
    pub thermal_max: f64,
    /// A pad is thermal when both its sides are at least this long (mm).
    pub thermal_side: f64,
    /// The longest side of a window (mm) and the least web between windows (mm).
    pub window_max: f64,
    pub web: f64,
}

impl Settings {
    pub fn from_config(c: &MachineConfig) -> Settings {
        Settings {
            thickness: c.stencil_thickness_mm,
            thickness_min: c.stencil_thickness_min_mm,
            thickness_max: c.stencil_thickness_max_mm,
            reduction: c.stencil_reduction_percent,
            reduction_min: c.stencil_reduction_min_percent,
            reduction_max: c.stencil_reduction_max_percent,
            fine_reduction: c.stencil_fine_reduction_percent,
            fine_min: c.stencil_fine_reduction_min_percent,
            fine_max: c.stencil_fine_reduction_max_percent,
            fine_pitch: c.stencil_fine_pitch_mm,
            area_ratio_min: c.stencil_area_ratio_min,
            thermal_percent: c.stencil_thermal_percent,
            thermal_min: c.stencil_thermal_min_percent,
            thermal_max: c.stencil_thermal_max_percent,
            thermal_side: c.stencil_thermal_side_mm,
            window_max: c.stencil_window_max_mm,
            web: c.stencil_web_mm,
        }
    }
}

/// One opening of the stencil.
#[derive(Debug, Clone, PartialEq)]
pub struct Aperture {
    /// The part the pad belongs to.
    pub id: String,
    /// `pad` or `window`.
    pub kind: &'static str,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// The area ratio of the opening.
    pub area_ratio: f64,
    /// The reduction that was applied to each side, per cent (0 for a window).
    pub reduction: f64,
}

/// What does not keep a limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    ThicknessRange,
    ReductionRange,
    FineReductionRange,
    ThermalRange,
    AreaRatio,
    WebNarrow,
}

impl Rule {
    pub fn name(self) -> &'static str {
        match self {
            Rule::ThicknessRange => "thickness_range",
            Rule::ReductionRange => "reduction_range",
            Rule::FineReductionRange => "fine_reduction_range",
            Rule::ThermalRange => "thermal_range",
            Rule::AreaRatio => "area_ratio",
            Rule::WebNarrow => "web_narrow",
        }
    }
}

/// A warning.
#[derive(Debug, Clone, PartialEq)]
pub struct Warning {
    pub rule: Rule,
    pub subject: String,
    pub value: f64,
    pub limit: f64,
}

/// The openings and the warnings.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stencil {
    pub apertures: Vec<Aperture>,
    pub warnings: Vec<Warning>,
}

/// The area ratio of an opening of `w` by `h` in a stencil `t` thick: its area over the area of its walls.
pub fn area_ratio(w: f64, h: f64, t: f64) -> f64 {
    if w <= 0.0 || h <= 0.0 || t <= 0.0 {
        return 0.0;
    }
    (w * h) / (2.0 * (w + h) * t)
}

/// The least distance between the centres of two pads of the same part and side, for each pad (infinite for a part with one pad).
fn pitches(pads: &[&Feature]) -> Vec<f64> {
    let mut by_part: std::collections::BTreeMap<(&str, bool), Vec<usize>> =
        std::collections::BTreeMap::new();
    for (i, p) in pads.iter().enumerate() {
        by_part
            .entry((p.id.as_str(), p.side == crate::parts::Side::Top))
            .or_default()
            .push(i);
    }
    let mut out = vec![f64::INFINITY; pads.len()];
    for list in by_part.values() {
        for &a in list {
            for &b in list {
                if a != b {
                    let d = (pads[a].x - pads[b].x).hypot(pads[a].y - pads[b].y);
                    out[a] = out[a].min(d);
                }
            }
        }
    }
    out
}

/// Makes the openings for the pads of the features and checks them.
pub fn make(features: &[Feature], s: &Settings) -> Stencil {
    let mut st = Stencil::default();
    let warn = |st: &mut Stencil, rule: Rule, subject: &str, value: f64, limit: f64| {
        st.warnings.push(Warning {
            rule,
            subject: subject.to_string(),
            value,
            limit,
        });
    };
    if !(s.thickness_min..=s.thickness_max).contains(&s.thickness) {
        let limit = if s.thickness < s.thickness_min {
            s.thickness_min
        } else {
            s.thickness_max
        };
        warn(&mut st, Rule::ThicknessRange, "-", s.thickness, limit);
    }
    let range = |v: f64, lo: f64, hi: f64| if v < lo { lo } else { hi };
    if !(s.reduction_min..=s.reduction_max).contains(&s.reduction) {
        warn(
            &mut st,
            Rule::ReductionRange,
            "-",
            s.reduction,
            range(s.reduction, s.reduction_min, s.reduction_max),
        );
    }
    if !(s.fine_min..=s.fine_max).contains(&s.fine_reduction) {
        warn(
            &mut st,
            Rule::FineReductionRange,
            "-",
            s.fine_reduction,
            range(s.fine_reduction, s.fine_min, s.fine_max),
        );
    }
    if !(s.thermal_min..=s.thermal_max).contains(&s.thermal_percent) {
        warn(
            &mut st,
            Rule::ThermalRange,
            "-",
            s.thermal_percent,
            range(s.thermal_percent, s.thermal_min, s.thermal_max),
        );
    }
    let pads: Vec<&Feature> = features
        .iter()
        .filter(|f| f.kind == Kind::Pad && f.w > 0.0 && f.h > 0.0)
        .take(MAX_APERTURES)
        .collect();
    let pitch = pitches(&pads);
    let mut ordered: Vec<usize> = (0..pads.len()).collect();
    ordered.sort_by(|&a, &b| pads[a].id.cmp(&pads[b].id).then(a.cmp(&b)));
    for i in ordered {
        let p = pads[i];
        if p.w >= s.thermal_side && p.h >= s.thermal_side {
            let cover = (s.thermal_percent / 100.0).clamp(0.01, 1.0);
            let nx = (p.w / s.window_max).ceil().max(1.0);
            let ny = (p.h / s.window_max).ceil().max(1.0);
            let (cx, cy) = (p.w / nx, p.h / ny);
            let scale = cover.sqrt();
            let (ww, wh) = (cx * scale, cy * scale);
            let web = (cx - ww).min(cy - wh);
            if web < s.web {
                warn(&mut st, Rule::WebNarrow, &p.id, web, s.web);
            }
            for ix in 0..nx as usize {
                for iy in 0..ny as usize {
                    let ar = area_ratio(ww, wh, s.thickness);
                    if ar < s.area_ratio_min {
                        warn(&mut st, Rule::AreaRatio, &p.id, ar, s.area_ratio_min);
                    }
                    st.apertures.push(Aperture {
                        id: p.id.clone(),
                        kind: "window",
                        x: p.x - p.w / 2.0 + cx * (ix as f64 + 0.5),
                        y: p.y - p.h / 2.0 + cy * (iy as f64 + 0.5),
                        w: ww,
                        h: wh,
                        area_ratio: ar,
                        reduction: 0.0,
                    });
                }
            }
        } else {
            let r = if pitch[i] <= s.fine_pitch {
                s.fine_reduction
            } else {
                s.reduction
            };
            let k = 1.0 - r / 100.0;
            let (w, h) = (p.w * k, p.h * k);
            let ar = area_ratio(w, h, s.thickness);
            if ar < s.area_ratio_min {
                warn(&mut st, Rule::AreaRatio, &p.id, ar, s.area_ratio_min);
            }
            st.apertures.push(Aperture {
                id: p.id.clone(),
                kind: "pad",
                x: p.x,
                y: p.y,
                w,
                h,
                area_ratio: ar,
                reduction: r,
            });
        }
    }
    st
}

fn index(lang: Lang) -> usize {
    Lang::ALL.iter().position(|l| *l == lang).unwrap_or(0)
}

fn number(v: f64) -> String {
    let t = format!("{v:.3}");
    t.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn template(rule: Rule) -> [&'static str; 8] {
    match rule {
        Rule::ThicknessRange => [
            "the stencil is {value} mm thick, outside the range that ends at {limit} mm",
            "la plantilla tiene {value} mm de espesor, fuera del rango que termina en {limit} mm",
            "die Schablone ist {value} mm dick, außerhalb des Bereichs, der bei {limit} mm endet",
            "le pochoir fait {value} mm d'épaisseur, hors de la plage qui s'arrête à {limit} mm",
            "lo stencil è spesso {value} mm, fuori dall'intervallo che finisce a {limit} mm",
            "ステンシルの厚さ {value} mm は、{limit} mm で終わる範囲の外です",
            "钢网厚度 {value} mm，超出了以 {limit} mm 为界的范围",
            "a máscara tem {value} mm de espessura, fora do intervalo que termina em {limit} mm",
        ],
        Rule::ReductionRange => [
            "the reduction of the openings is {value} per cent, outside the range that ends at {limit}",
            "la reducción de las aperturas es del {value} %, fuera del rango que termina en {limit}",
            "die Verkleinerung der Öffnungen beträgt {value} %, außerhalb des Bereichs, der bei {limit} endet",
            "la réduction des ouvertures est de {value} %, hors de la plage qui s'arrête à {limit}",
            "la riduzione delle aperture è del {value} %, fuori dall'intervallo che finisce a {limit}",
            "開口の縮小率 {value} % は、{limit} で終わる範囲の外です",
            "开孔缩减 {value} %，超出了以 {limit} 为界的范围",
            "a redução das aberturas é de {value} %, fora do intervalo que termina em {limit}",
        ],
        Rule::FineReductionRange => [
            "the reduction for fine pitch is {value} per cent, outside the range that ends at {limit}",
            "la reducción para paso fino es del {value} %, fuera del rango que termina en {limit}",
            "die Verkleinerung für Fine Pitch beträgt {value} %, außerhalb des Bereichs, der bei {limit} endet",
            "la réduction pour le pas fin est de {value} %, hors de la plage qui s'arrête à {limit}",
            "la riduzione per il passo fine è del {value} %, fuori dall'intervallo che finisce a {limit}",
            "ファインピッチの縮小率 {value} % は、{limit} で終わる範囲の外です",
            "细间距的缩减 {value} %，超出了以 {limit} 为界的范围",
            "a redução para passo fino é de {value} %, fora do intervalo que termina em {limit}",
        ],
        Rule::ThermalRange => [
            "the windows cover {value} per cent of the thermal pad, outside the range that ends at {limit}",
            "las ventanas cubren el {value} % del pad térmico, fuera del rango que termina en {limit}",
            "die Fenster decken {value} % des Wärmepads ab, außerhalb des Bereichs, der bei {limit} endet",
            "les fenêtres couvrent {value} % de la pastille thermique, hors de la plage qui s'arrête à {limit}",
            "le finestre coprono il {value} % del pad termico, fuori dall'intervallo che finisce a {limit}",
            "ウィンドウが放熱パッドの {value} % を覆っており、{limit} で終わる範囲の外です",
            "窗口覆盖散热焊盘的 {value} %，超出了以 {limit} 为界的范围",
            "as janelas cobrem {value} % do pad térmico, fora do intervalo que termina em {limit}",
        ],
        Rule::AreaRatio => [
            "{subject}: the area ratio of an opening is {value}, under {limit}: the paste may stay in the walls",
            "{subject}: la relación de área de una apertura es {value}, menos de {limit}: la pasta puede quedarse en las paredes",
            "{subject}: das Flächenverhältnis einer Öffnung ist {value}, unter {limit}: die Paste kann in den Wänden haften",
            "{subject} : le rapport de surface d'une ouverture est {value}, sous {limit} : la pâte peut rester sur les parois",
            "{subject}: il rapporto d'area di un'apertura è {value}, sotto {limit}: la pasta può restare sulle pareti",
            "{subject}：開口の面積比は {value} で {limit} を下回り、ペーストが壁に残るおそれがあります",
            "{subject}：开孔面积比为 {value}，低于 {limit}：锡膏可能残留在孔壁",
            "{subject}: a razão de área de uma abertura é {value}, abaixo de {limit}: a pasta pode ficar nas paredes",
        ],
        Rule::WebNarrow => [
            "{subject}: the web between windows is {value} mm, under {limit} mm",
            "{subject}: el puente entre ventanas es de {value} mm, menos de {limit} mm",
            "{subject}: der Steg zwischen den Fenstern ist {value} mm, unter {limit} mm",
            "{subject} : le pont entre fenêtres est de {value} mm, moins de {limit} mm",
            "{subject}: il ponte tra le finestre è di {value} mm, meno di {limit} mm",
            "{subject}：ウィンドウ間の桟は {value} mm で、{limit} mm に満たない",
            "{subject}：窗口之间的间隔为 {value} mm，小于 {limit} mm",
            "{subject}: a ponte entre janelas é de {value} mm, abaixo de {limit} mm",
        ],
    }
}

/// The message of a warning in a language.
pub fn message(w: &Warning, lang: Lang) -> String {
    template(w.rule)[index(lang)]
        .replace("{subject}", &w.subject)
        .replace("{value}", &number(w.value))
        .replace("{limit}", &number(w.limit))
}

impl Stencil {
    /// The report for a person: the warnings, then one line that counts the openings.
    pub fn text(&self, lang: Lang) -> String {
        let mut out = String::new();
        for w in &self.warnings {
            out.push_str(&format!("[warning] {}\n", message(w, lang)));
        }
        let windows = self.apertures.iter().filter(|a| a.kind == "window").count();
        let line: [&str; 8] = [
            "{n} openings ({w} of them windows of thermal pads), {k} warnings",
            "{n} aperturas ({w} de ellas ventanas de pads térmicos), {k} avisos",
            "{n} Öffnungen ({w} davon Fenster von Wärmepads), {k} Warnungen",
            "{n} ouvertures ({w} fenêtres de pastilles thermiques), {k} avertissements",
            "{n} aperture ({w} finestre di pad termici), {k} avvisi",
            "開口 {n} 個（うち放熱パッドのウィンドウ {w} 個）、警告 {k} 件",
            "{n} 个开孔（其中散热焊盘窗口 {w} 个），{k} 条警告",
            "{n} aberturas ({w} delas janelas de pads térmicos), {k} avisos",
        ];
        out.push_str(
            &line[index(lang)]
                .replace("{n}", &self.apertures.len().to_string())
                .replace("{w}", &windows.to_string())
                .replace("{k}", &self.warnings.len().to_string()),
        );
        out.push('\n');
        out
    }

    /// The openings as a CSV: `id,kind,x_mm,y_mm,w_mm,h_mm,area_ratio,reduction_percent`.
    pub fn csv(&self) -> String {
        let mut out = String::from("id,kind,x_mm,y_mm,w_mm,h_mm,area_ratio,reduction_percent\n");
        for a in &self.apertures {
            out.push_str(&format!(
                "{},{},{:.4},{:.4},{:.4},{:.4},{:.3},{}\n",
                csv_field(&a.id),
                a.kind,
                a.x,
                a.y,
                a.w,
                a.h,
                a.area_ratio,
                number(a.reduction)
            ));
        }
        out
    }
}
