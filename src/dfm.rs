// EH3D-PHOENIX-PNP - the checks of a board for assembly (design for manufacturing), with the limits in the settings.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A board that is fine on the screen may be hard to assemble: pads too near one another, bodies that touch, a hole inside a pad that is to be soldered, a track that ends in the middle of a
//! pad, fiducials and tooling holes where the machine cannot use them, a tab that is too narrow. This reads the parts, a table of the features that the parts do not say (the outline,
//! the pads, the holes, the ends of tracks, the fiducials, the tooling holes and the tabs) and the table of packages, and gives a finding for each limit that is not kept, in the eight
//! languages. Every limit is a setting (`dfm_*`); the defaults are the public figures of the guides of assembly houses, and none of them has been compared with the rules of a particular
//! house, which the report says for as long as `dfm_verified` is off. It reads and says; it changes nothing.

use crate::catalog::Catalog;
use crate::clearance;
use crate::config::MachineConfig;
use crate::error::{Code, Lang, PnpError};
use crate::jlcpcb;
use crate::parts::{csv_field, detect_delimiter, read_records_numbered, Part, Side};

/// The most features a table may hold.
pub const MAX_FEATURES: usize = 200_000;
/// The most findings that are kept (the report says when there were more).
pub const MAX_FINDINGS: usize = 5_000;
/// A figure within this of its limit keeps the limit (the sums of floating point numbers are not exact).
const EPS: f64 = 1e-9;

/// The kinds of feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The outline of the board or the panel: the lower left corner and the size.
    Outline,
    /// A pad that is soldered on the surface: the centre and the size.
    Pad,
    /// A hole: the centre and the diameter (`w_mm`).
    Hole,
    /// The end of a track on a pad: the place and the width (`w_mm`).
    Track,
    /// A fiducial: the centre and the diameter.
    Fiducial,
    /// A tooling hole: the centre and the diameter.
    Tooling,
    /// A tab or rail of the panel: the place and the width (`w_mm`).
    Tab,
}

impl Kind {
    pub fn parse(text: &str) -> Option<Kind> {
        match text.trim().to_ascii_lowercase().as_str() {
            "outline" => Some(Kind::Outline),
            "pad" => Some(Kind::Pad),
            "hole" => Some(Kind::Hole),
            "track" => Some(Kind::Track),
            "fiducial" => Some(Kind::Fiducial),
            "tooling" => Some(Kind::Tooling),
            "tab" => Some(Kind::Tab),
            _ => None,
        }
    }
}

/// One row of the table of features.
#[derive(Debug, Clone, PartialEq)]
pub struct Feature {
    pub kind: Kind,
    /// The part the pad belongs to, or a name for the feature.
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub side: Side,
}

fn bad(line: usize, what: impl Into<String>) -> PnpError {
    PnpError::new(Code::ImportBad, format!("line {line}: {}", what.into()))
}

/// Reads the table of features: `kind,id,x_mm,y_mm,w_mm,h_mm,side` (the columns are found by name; `h_mm` and `side` may be left out, `h_mm` then being `w_mm`, `side` then `top`).
pub fn parse_features(text: &str) -> Result<Vec<Feature>, PnpError> {
    let text = text.trim_start_matches('\u{feff}');
    let head = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let records =
        read_records_numbered(text, detect_delimiter(head)).map_err(|(line, e)| bad(line, e))?;
    let mut rows = records.into_iter();
    let (hline, header) = rows.next().ok_or_else(|| bad(1, "the file is empty"))?;
    let keys: Vec<String> = header
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();
    let find = |names: &[&str]| keys.iter().position(|k| names.contains(&k.as_str()));
    let kind_at =
        find(&["kind", "type"]).ok_or_else(|| bad(hline, "the header has no column kind"))?;
    let id_at = find(&["id", "ref", "reference", "name"]);
    let x_at = find(&["x_mm", "x"]).ok_or_else(|| bad(hline, "the header has no column x_mm"))?;
    let y_at = find(&["y_mm", "y"]).ok_or_else(|| bad(hline, "the header has no column y_mm"))?;
    let w_at = find(&["w_mm", "w", "d_mm", "d", "diameter"])
        .ok_or_else(|| bad(hline, "the header has no column w_mm"))?;
    let h_at = find(&["h_mm", "h"]);
    let side_at = find(&["side", "layer"]);
    let mut out = Vec::new();
    for (line, row) in rows {
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        if out.len() >= MAX_FEATURES {
            return Err(bad(line, format!("more than {MAX_FEATURES} features")));
        }
        let cell = |i: usize| row.get(i).map_or("", |s| s.trim());
        let kind = Kind::parse(cell(kind_at)).ok_or_else(|| {
            bad(
                line,
                "the kind is outline, pad, hole, track, fiducial, tooling or tab",
            )
        })?;
        let number = |i: usize, what: &str, lo: f64| -> Result<f64, PnpError> {
            cell(i)
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && v.abs() <= 100_000.0 && *v >= lo)
                .ok_or_else(|| bad(line, format!("{what} is a number of millimetres")))
        };
        let w = number(w_at, "w_mm", 0.0)?;
        let h = match h_at {
            Some(i) if !cell(i).is_empty() => number(i, "h_mm", 0.0)?,
            _ => w,
        };
        let side = match side_at.map(cell) {
            None | Some("") => Side::Top,
            Some(s) => match s.to_ascii_lowercase().as_str() {
                "top" | "t" | "front" | "f" => Side::Top,
                "bottom" | "b" | "back" => Side::Bottom,
                _ => return Err(bad(line, "the side is top or bottom")),
            },
        };
        out.push(Feature {
            kind,
            id: id_at.map(cell).unwrap_or("").to_string(),
            x: number(x_at, "x_mm", -100_000.0)?,
            y: number(y_at, "y_mm", -100_000.0)?,
            w,
            h,
            side,
        });
    }
    Ok(out)
}

/// The limits, in millimetres (the settings `dfm_*`).
#[derive(Debug, Clone, PartialEq)]
pub struct Limits {
    pub pad_pitch: f64,
    pub pad_gap_min: f64,
    pub smt_gap: f64,
    pub track_center_frac: f64,
    pub tooling_edge: f64,
    pub smt_tooling_edge: f64,
    pub tooling_hole: f64,
    pub hole_tol: f64,
    pub fid_diameter: f64,
    pub fid_edge: f64,
    pub fid_ring: f64,
    pub tab_min: f64,
    /// Whether the limits have been compared with the rules of the house that assembles the board.
    pub verified: bool,
    /// The words that tell a part that goes through the board (it is not judged as a part on the surface).
    pub tht_words: Vec<String>,
}

impl Limits {
    pub fn from_config(cfg: &MachineConfig) -> Limits {
        Limits {
            pad_pitch: cfg.dfm_pad_pitch_mm,
            pad_gap_min: cfg.dfm_pad_gap_min_mm,
            smt_gap: cfg.dfm_smt_gap_mm,
            track_center_frac: cfg.dfm_track_center_frac,
            tooling_edge: cfg.dfm_tooling_edge_mm,
            smt_tooling_edge: cfg.dfm_smt_tooling_edge_mm,
            tooling_hole: cfg.dfm_tooling_hole_mm,
            hole_tol: cfg.dfm_hole_tol_mm,
            fid_diameter: cfg.dfm_fid_diameter_mm,
            fid_edge: cfg.dfm_fid_edge_mm,
            fid_ring: cfg.dfm_fid_ring_mm,
            tab_min: cfg.dfm_rail_min_mm,
            verified: cfg.dfm_verified,
            tht_words: cfg.fab_tht_words.clone(),
        }
    }
}

/// The checks, each with a name that is stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rule {
    PadPitch,
    PadGap,
    SmtGap,
    HoleInPad,
    TrackCenter,
    ToolingSize,
    ToolingEdge,
    SmtToolingEdge,
    FiducialSize,
    FiducialEdge,
    FiducialRing,
    TabWidth,
    NoOutline,
}

impl Rule {
    pub const ALL: [Rule; 13] = [
        Rule::PadPitch,
        Rule::PadGap,
        Rule::SmtGap,
        Rule::HoleInPad,
        Rule::TrackCenter,
        Rule::ToolingSize,
        Rule::ToolingEdge,
        Rule::SmtToolingEdge,
        Rule::FiducialSize,
        Rule::FiducialEdge,
        Rule::FiducialRing,
        Rule::TabWidth,
        Rule::NoOutline,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Rule::PadPitch => "pad_pitch",
            Rule::PadGap => "pad_gap",
            Rule::SmtGap => "smt_gap",
            Rule::HoleInPad => "hole_in_pad",
            Rule::TrackCenter => "track_center",
            Rule::ToolingSize => "tooling_size",
            Rule::ToolingEdge => "tooling_edge",
            Rule::SmtToolingEdge => "smt_tooling_edge",
            Rule::FiducialSize => "fiducial_size",
            Rule::FiducialEdge => "fiducial_edge",
            Rule::FiducialRing => "fiducial_ring",
            Rule::TabWidth => "tab_width",
            Rule::NoOutline => "no_outline",
        }
    }

    /// How serious a finding of this rule is.
    pub fn severity(self) -> Severity {
        match self {
            Rule::HoleInPad => Severity::Error,
            Rule::PadPitch | Rule::NoOutline => Severity::Note,
            _ => Severity::Warning,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Note,
    Warning,
    Error,
}

impl Severity {
    pub fn name(self) -> &'static str {
        match self {
            Severity::Note => "note",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

/// A limit that is not kept.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub rule: Rule,
    pub subject: String,
    /// What was measured and the limit, in millimetres (`NaN` when the rule has none).
    pub value: f64,
    pub limit: f64,
    pub x: f64,
    pub y: f64,
}

/// What the check found.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
    /// Whether there were more findings than `MAX_FINDINGS`.
    pub truncated: bool,
    pub verified: bool,
    /// The parts, pads and fiducials that were looked at.
    pub parts: usize,
    pub pads: usize,
}

struct Rect {
    id: String,
    side: Side,
    x: f64,
    y: f64,
    hx: f64,
    hy: f64,
}

fn rect_gap(a: &Rect, b: &Rect) -> f64 {
    clearance::gap((a.x, a.y, a.hx, a.hy), (b.x, b.y, b.hx, b.hy))
}

/// The distance from a point to the nearest edge of the outline (negative outside), and the edge: 0 left, 1 right, 2 bottom, 3 top.
fn edge_distance(o: &Feature, x: f64, y: f64) -> (f64, usize) {
    let d = [x - o.x, o.x + o.w - x, y - o.y, o.y + o.h - y];
    d.iter()
        .copied()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map_or((f64::NAN, 0), |(i, v)| (v, i))
}

/// Runs the checks. `catalog` gives the bodies of the parts for the space between parts (a part with no body size is not compared).
pub fn check(parts: &[Part], features: &[Feature], catalog: &Catalog, limits: &Limits) -> Report {
    let mut r = Report {
        verified: limits.verified,
        parts: parts.len(),
        ..Report::default()
    };
    let mut add = |rule: Rule, subject: &str, value: f64, limit: f64, x: f64, y: f64| {
        if r.findings.len() >= MAX_FINDINGS {
            r.truncated = true;
        } else {
            r.findings.push(Finding {
                rule,
                subject: subject.to_string(),
                value,
                limit,
                x,
                y,
            });
        }
    };
    let pads: Vec<&Feature> = features.iter().filter(|f| f.kind == Kind::Pad).collect();
    let rects: Vec<Rect> = pads
        .iter()
        .map(|f| Rect {
            id: f.id.clone(),
            side: f.side,
            x: f.x,
            y: f.y,
            hx: f.w / 2.0,
            hy: f.h / 2.0,
        })
        .collect();

    // the pitch of the pads of one part: the nearest other pad of the same part
    let mut by_part: std::collections::BTreeMap<&str, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (i, f) in pads.iter().enumerate() {
        by_part.entry(f.id.as_str()).or_default().push(i);
    }
    for (id, list) in &by_part {
        let mut least = f64::INFINITY;
        for (n, &a) in list.iter().enumerate() {
            for &b in &list[n + 1..] {
                if pads[a].side == pads[b].side {
                    least = least.min((pads[a].x - pads[b].x).hypot(pads[a].y - pads[b].y));
                }
            }
        }
        if least.is_finite() && least > 0.0 && least <= limits.pad_pitch + EPS {
            let first = pads[list[0]];
            add(
                Rule::PadPitch,
                id,
                least,
                limits.pad_pitch,
                first.x,
                first.y,
            );
        }
    }

    // the gap between the pads of two parts, with a sweep over the left edges
    let mut order: Vec<usize> = (0..rects.len()).collect();
    order.sort_by(|&a, &b| (rects[a].x - rects[a].hx).total_cmp(&(rects[b].x - rects[b].hx)));
    for (n, &a) in order.iter().enumerate() {
        for &b in &order[n + 1..] {
            let (ra, rb) = (&rects[a], &rects[b]);
            if rb.x - rb.hx - (ra.x + ra.hx) >= limits.pad_gap_min {
                break;
            }
            if ra.side != rb.side || ra.id == rb.id {
                continue;
            }
            let g = rect_gap(ra, rb);
            if g < limits.pad_gap_min - EPS {
                let subject = if ra.id <= rb.id {
                    format!("{} / {}", ra.id, rb.id)
                } else {
                    format!("{} / {}", rb.id, ra.id)
                };
                add(Rule::PadGap, &subject, g, limits.pad_gap_min, ra.x, ra.y);
            }
        }
    }

    // the space between the bodies of the parts on the surface
    let smt: Vec<Part> = parts
        .iter()
        .filter(|p| !jlcpcb::is_tht(p, &limits.tht_words))
        .cloned()
        .collect();
    for pair in clearance::check(&smt, catalog, limits.smt_gap).pairs {
        let at = parts
            .iter()
            .find(|p| p.reference == pair.first)
            .map_or((0.0, 0.0), |p| (p.x, p.y));
        add(
            Rule::SmtGap,
            &format!("{} / {}", pair.first, pair.second),
            pair.gap_mm,
            limits.smt_gap,
            at.0,
            at.1,
        );
    }

    // a hole inside a pad that is soldered; the end of a track in the middle of a pad
    for h in features.iter().filter(|f| f.kind == Kind::Hole) {
        for p in &pads {
            if (h.x - p.x).abs() <= p.w / 2.0 && (h.y - p.y).abs() <= p.h / 2.0 {
                add(Rule::HoleInPad, &p.id, 0.0, f64::NAN, h.x, h.y);
            }
        }
    }
    for t in features.iter().filter(|f| f.kind == Kind::Track) {
        for p in &pads {
            if p.side != t.side {
                continue;
            }
            let reach = limits.track_center_frac * (p.w.min(p.h) / 2.0);
            let d = (t.x - p.x).hypot(t.y - p.y);
            if d <= reach + EPS {
                add(Rule::TrackCenter, &p.id, d, reach, p.x, p.y);
            }
        }
    }

    // the edges: the outline is needed
    let outline = features.iter().find(|f| f.kind == Kind::Outline);
    let needs_edges = features
        .iter()
        .any(|f| matches!(f.kind, Kind::Tooling | Kind::Fiducial | Kind::Tab));
    if outline.is_none() && needs_edges {
        add(Rule::NoOutline, "-", f64::NAN, f64::NAN, 0.0, 0.0);
    }
    let mut tooling_edges = [false; 4];
    for t in features.iter().filter(|f| f.kind == Kind::Tooling) {
        if (t.w - limits.tooling_hole).abs() > limits.hole_tol {
            add(Rule::ToolingSize, &t.id, t.w, limits.tooling_hole, t.x, t.y);
        }
        if let Some(o) = outline {
            let (d, edge) = edge_distance(o, t.x, t.y);
            tooling_edges[edge] = true;
            if d < limits.tooling_edge - EPS {
                add(Rule::ToolingEdge, &t.id, d, limits.tooling_edge, t.x, t.y);
            }
        }
    }
    if let Some(o) = outline {
        for p in &smt {
            let from_edge = |edge: usize| match edge {
                0 => p.x - o.x,
                1 => o.x + o.w - p.x,
                2 => p.y - o.y,
                _ => o.y + o.h - p.y,
            };
            for (edge, _) in tooling_edges.iter().enumerate().filter(|(_, on)| **on) {
                let d = from_edge(edge);
                if d < limits.smt_tooling_edge - EPS {
                    add(
                        Rule::SmtToolingEdge,
                        &p.reference,
                        d,
                        limits.smt_tooling_edge,
                        p.x,
                        p.y,
                    );
                    break;
                }
            }
        }
        for f in features.iter().filter(|f| f.kind == Kind::Fiducial) {
            let (d, _) = edge_distance(o, f.x, f.y);
            if d < limits.fid_edge - EPS {
                add(Rule::FiducialEdge, &f.id, d, limits.fid_edge, f.x, f.y);
            }
        }
    }
    for f in features.iter().filter(|f| f.kind == Kind::Fiducial) {
        if (f.w - limits.fid_diameter).abs() > limits.hole_tol {
            add(
                Rule::FiducialSize,
                &f.id,
                f.w,
                limits.fid_diameter,
                f.x,
                f.y,
            );
        }
        let near = pads
            .iter()
            .map(|p| (p.x - f.x).hypot(p.y - f.y))
            .chain(
                features
                    .iter()
                    .filter(|h| h.kind == Kind::Hole)
                    .map(|h| (h.x - f.x).hypot(h.y - f.y)),
            )
            .fold(f64::INFINITY, f64::min);
        if near < limits.fid_ring - EPS {
            add(Rule::FiducialRing, &f.id, near, limits.fid_ring, f.x, f.y);
        }
    }
    for t in features.iter().filter(|f| f.kind == Kind::Tab) {
        if t.w < limits.tab_min - EPS {
            add(Rule::TabWidth, &t.id, t.w, limits.tab_min, t.x, t.y);
        }
    }
    r.pads = pads.len();
    r.findings.sort_by(|a, b| {
        b.rule
            .severity()
            .cmp(&a.rule.severity())
            .then_with(|| a.rule.cmp(&b.rule))
            .then_with(|| a.subject.cmp(&b.subject))
    });
    r
}

fn index(lang: Lang) -> usize {
    Lang::ALL.iter().position(|l| *l == lang).unwrap_or(0)
}

/// The text of a rule in the eight languages, in the order of `Lang::ALL`; `{subject}`, `{value}` and `{limit}` are filled in.
fn template(rule: Rule) -> [&'static str; 8] {
    match rule {
        Rule::PadPitch => [
            "{subject}: pads {value} mm apart, at or under {limit} mm: fine pitch, it needs a stencil with smaller apertures",
            "{subject}: pads a {value} mm, igual o menos que {limit} mm: paso fino, pide una plantilla con aperturas más pequeñas",
            "{subject}: Pads {value} mm auseinander, höchstens {limit} mm: Fine Pitch, die Schablone braucht kleinere Öffnungen",
            "{subject} : pastilles à {value} mm, au plus {limit} mm : pas fin, le pochoir demande des ouvertures plus petites",
            "{subject}: pad a {value} mm, al massimo {limit} mm: passo fine, serve uno stencil con aperture più piccole",
            "{subject}：パッド間隔 {value} mm（{limit} mm 以下）：ファインピッチのため、開口を小さくしたステンシルが必要です",
            "{subject}：焊盘间距 {value} mm，不大于 {limit} mm：细间距，钢网开孔需要缩小",
            "{subject}: pads a {value} mm, no máximo {limit} mm: passo fino, a máscara precisa de aberturas mais pequenas",
        ],
        Rule::PadGap => [
            "{subject}: the gap between pads is {value} mm, under the {limit} mm that is asked for",
            "{subject}: el hueco entre pads es de {value} mm, menos de los {limit} mm pedidos",
            "{subject}: der Abstand zwischen den Pads beträgt {value} mm, unter den geforderten {limit} mm",
            "{subject} : l'espace entre pastilles est de {value} mm, moins des {limit} mm demandés",
            "{subject}: lo spazio tra i pad è {value} mm, meno dei {limit} mm richiesti",
            "{subject}：パッド間の隙間は {value} mm で、要求される {limit} mm に満たない",
            "{subject}：焊盘间隙为 {value} mm，小于要求的 {limit} mm",
            "{subject}: o espaço entre pads é de {value} mm, abaixo dos {limit} mm pedidos",
        ],
        Rule::SmtGap => [
            "{subject}: the bodies are {value} mm apart, under the {limit} mm between parts on the surface",
            "{subject}: los cuerpos están a {value} mm, menos de los {limit} mm entre piezas de superficie",
            "{subject}: die Gehäuse sind {value} mm voneinander entfernt, unter den {limit} mm zwischen SMD-Bauteilen",
            "{subject} : les corps sont à {value} mm, moins des {limit} mm entre composants de surface",
            "{subject}: i corpi distano {value} mm, meno dei {limit} mm tra componenti a montaggio superficiale",
            "{subject}：部品本体の間隔は {value} mm で、表面実装部品間の {limit} mm に満たない",
            "{subject}：元件本体间距为 {value} mm，小于贴片元件之间的 {limit} mm",
            "{subject}: os corpos estão a {value} mm, abaixo dos {limit} mm entre componentes de superfície",
        ],
        Rule::HoleInPad => [
            "{subject}: a hole lies inside a pad that is soldered on the surface",
            "{subject}: hay un agujero dentro de un pad que se suelda en superficie",
            "{subject}: ein Loch liegt in einem Pad, das auf der Oberfläche gelötet wird",
            "{subject} : un trou se trouve dans une pastille soudée en surface",
            "{subject}: un foro si trova dentro un pad saldato in superficie",
            "{subject}：表面実装のパッドの内側に穴があります",
            "{subject}：表面贴装焊盘内有孔",
            "{subject}: há um furo dentro de um pad soldado à superfície",
        ],
        Rule::TrackCenter => [
            "{subject}: a track ends {value} mm from the middle of the pad (limit {limit} mm): it can draw the solder away",
            "{subject}: una pista termina a {value} mm del centro del pad (límite {limit} mm): puede arrastrar la soldadura",
            "{subject}: eine Leiterbahn endet {value} mm von der Padmitte (Grenze {limit} mm): sie kann das Lot abziehen",
            "{subject} : une piste se termine à {value} mm du centre de la pastille (limite {limit} mm) : elle peut attirer la soudure",
            "{subject}: una pista termina a {value} mm dal centro del pad (limite {limit} mm): può richiamare la saldatura",
            "{subject}：配線がパッド中心から {value} mm で終わっています（上限 {limit} mm）：はんだを引き込むおそれがあります",
            "{subject}：走线在距焊盘中心 {value} mm 处结束（限值 {limit} mm）：可能把焊锡拉走",
            "{subject}: uma pista termina a {value} mm do centro do pad (limite {limit} mm): pode puxar a soldadura",
        ],
        Rule::ToolingSize => [
            "{subject}: the tooling hole is {value} mm across, not the {limit} mm that is asked for",
            "{subject}: el agujero de herramienta mide {value} mm, no los {limit} mm pedidos",
            "{subject}: das Werkzeugloch hat {value} mm Durchmesser, nicht die geforderten {limit} mm",
            "{subject} : le trou d'outillage mesure {value} mm, pas les {limit} mm demandés",
            "{subject}: il foro di riferimento è di {value} mm, non dei {limit} mm richiesti",
            "{subject}：ツーリング穴の径は {value} mm で、要求の {limit} mm ではありません",
            "{subject}：工艺孔直径为 {value} mm，不是要求的 {limit} mm",
            "{subject}: o furo de ferramenta mede {value} mm, não os {limit} mm pedidos",
        ],
        Rule::ToolingEdge => [
            "{subject}: the tooling hole is {value} mm from the edge, under {limit} mm",
            "{subject}: el agujero de herramienta está a {value} mm del borde, menos de {limit} mm",
            "{subject}: das Werkzeugloch ist {value} mm vom Rand entfernt, unter {limit} mm",
            "{subject} : le trou d'outillage est à {value} mm du bord, moins de {limit} mm",
            "{subject}: il foro di riferimento è a {value} mm dal bordo, meno di {limit} mm",
            "{subject}：ツーリング穴は端から {value} mm で、{limit} mm に満たない",
            "{subject}：工艺孔距边缘 {value} mm，小于 {limit} mm",
            "{subject}: o furo de ferramenta está a {value} mm da borda, abaixo de {limit} mm",
        ],
        Rule::SmtToolingEdge => [
            "{subject}: the part is {value} mm from the edge with the tooling holes, under {limit} mm",
            "{subject}: la pieza está a {value} mm del borde con los agujeros de herramienta, menos de {limit} mm",
            "{subject}: das Bauteil ist {value} mm vom Rand mit den Werkzeuglöchern entfernt, unter {limit} mm",
            "{subject} : le composant est à {value} mm du bord des trous d'outillage, moins de {limit} mm",
            "{subject}: il componente è a {value} mm dal bordo con i fori di riferimento, meno di {limit} mm",
            "{subject}：部品はツーリング穴のある端から {value} mm で、{limit} mm に満たない",
            "{subject}：元件距工艺孔所在边 {value} mm，小于 {limit} mm",
            "{subject}: o componente está a {value} mm da borda dos furos de ferramenta, abaixo de {limit} mm",
        ],
        Rule::FiducialSize => [
            "{subject}: the fiducial is {value} mm across, not the {limit} mm that is asked for",
            "{subject}: el fiducial mide {value} mm, no los {limit} mm pedidos",
            "{subject}: die Passmarke hat {value} mm Durchmesser, nicht die geforderten {limit} mm",
            "{subject} : le repère mesure {value} mm, pas les {limit} mm demandés",
            "{subject}: il fiduciale è di {value} mm, non dei {limit} mm richiesti",
            "{subject}：フィデューシャルの径は {value} mm で、要求の {limit} mm ではありません",
            "{subject}：基准点直径为 {value} mm，不是要求的 {limit} mm",
            "{subject}: o fiducial mede {value} mm, não os {limit} mm pedidos",
        ],
        Rule::FiducialEdge => [
            "{subject}: the fiducial is {value} mm from the edge, under {limit} mm",
            "{subject}: el fiducial está a {value} mm del borde, menos de {limit} mm",
            "{subject}: die Passmarke ist {value} mm vom Rand entfernt, unter {limit} mm",
            "{subject} : le repère est à {value} mm du bord, moins de {limit} mm",
            "{subject}: il fiduciale è a {value} mm dal bordo, meno di {limit} mm",
            "{subject}：フィデューシャルは端から {value} mm で、{limit} mm に満たない",
            "{subject}：基准点距边缘 {value} mm，小于 {limit} mm",
            "{subject}: o fiducial está a {value} mm da borda, abaixo de {limit} mm",
        ],
        Rule::FiducialRing => [
            "{subject}: a pad or a hole is {value} mm from the fiducial, inside the free ring of {limit} mm",
            "{subject}: hay un pad o un agujero a {value} mm del fiducial, dentro del anillo libre de {limit} mm",
            "{subject}: ein Pad oder Loch liegt {value} mm von der Passmarke, im freien Ring von {limit} mm",
            "{subject} : une pastille ou un trou est à {value} mm du repère, dans l'anneau libre de {limit} mm",
            "{subject}: un pad o un foro è a {value} mm dal fiduciale, dentro l'anello libero di {limit} mm",
            "{subject}：パッドまたは穴がフィデューシャルから {value} mm にあり、空けるべき {limit} mm の輪の内側です",
            "{subject}：焊盘或孔距基准点 {value} mm，位于 {limit} mm 的净空环内",
            "{subject}: há um pad ou um furo a {value} mm do fiducial, dentro do anel livre de {limit} mm",
        ],
        Rule::TabWidth => [
            "{subject}: the tab is {value} mm wide, under {limit} mm",
            "{subject}: la pestaña mide {value} mm de ancho, menos de {limit} mm",
            "{subject}: der Steg ist {value} mm breit, unter {limit} mm",
            "{subject} : la languette fait {value} mm de large, moins de {limit} mm",
            "{subject}: la linguetta è larga {value} mm, meno di {limit} mm",
            "{subject}：タブの幅は {value} mm で、{limit} mm に満たない",
            "{subject}：连接片宽 {value} mm，小于 {limit} mm",
            "{subject}: a patilha tem {value} mm de largura, abaixo de {limit} mm",
        ],
        Rule::NoOutline => [
            "the table has no outline: the checks of the edges were left out",
            "la tabla no tiene contorno: se omitieron las comprobaciones de los bordes",
            "die Tabelle hat keine Umrisslinie: die Kantenprüfungen entfallen",
            "le tableau n'a pas de contour : les contrôles des bords sont omis",
            "la tabella non ha il contorno: i controlli dei bordi sono omessi",
            "表に外形がないため、端のチェックは行いませんでした",
            "表中没有外形：已跳过边缘检查",
            "a tabela não tem contorno: as verificações das bordas foram omitidas",
        ],
    }
}

fn number(v: f64) -> String {
    if v.is_finite() {
        let t = format!("{v:.3}");
        t.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        "-".to_string()
    }
}

/// The message of one finding in a language.
pub fn message(f: &Finding, lang: Lang) -> String {
    template(f.rule)[index(lang)]
        .replace("{subject}", &f.subject)
        .replace("{value}", &number(f.value))
        .replace("{limit}", &number(f.limit))
}

/// The line that says the limits are not verified (or that they are).
fn verified_line(lang: Lang, verified: bool) -> &'static str {
    let off: [&str; 8] = [
        "The limits are the public figures of assembly guides and have not been checked against the rules of a house (dfm_verified = off).",
        "Los límites son las cifras públicas de guías de montaje y no se han comprobado con las reglas de una casa (dfm_verified = off).",
        "Die Grenzen sind die öffentlichen Zahlen von Montageleitfäden und wurden nicht an den Regeln eines Fertigers geprüft (dfm_verified = off).",
        "Les limites sont les chiffres publics de guides d'assemblage et n'ont pas été comparées aux règles d'un assembleur (dfm_verified = off).",
        "I limiti sono le cifre pubbliche delle guide di assemblaggio e non sono stati confrontati con le regole di un fornitore (dfm_verified = off).",
        "上限は組立ガイドの公開値で、特定の業者の規則とは照合していません（dfm_verified = off）。",
        "限值取自装配指南的公开数字，尚未与具体厂家的规则核对（dfm_verified = off）。",
        "Os limites são os valores públicos de guias de montagem e não foram comparados com as regras de uma casa (dfm_verified = off).",
    ];
    let on: [&str; 8] = [
        "The limits were checked against the rules of the house (dfm_verified = on).",
        "Los límites se comprobaron con las reglas de la casa (dfm_verified = on).",
        "Die Grenzen wurden an den Regeln des Fertigers geprüft (dfm_verified = on).",
        "Les limites ont été comparées aux règles de l'assembleur (dfm_verified = on).",
        "I limiti sono stati confrontati con le regole del fornitore (dfm_verified = on).",
        "上限は業者の規則と照合済みです（dfm_verified = on）。",
        "限值已与厂家的规则核对（dfm_verified = on）。",
        "Os limites foram comparados com as regras da casa (dfm_verified = on).",
    ];
    let i = index(lang);
    if verified {
        on[i]
    } else {
        off[i]
    }
}

fn summary_line(lang: Lang, r: &Report) -> String {
    let count = |s: Severity| r.findings.iter().filter(|f| f.rule.severity() == s).count();
    let (e, w, n) = (
        count(Severity::Error),
        count(Severity::Warning),
        count(Severity::Note),
    );
    let t: [&str; 8] = [
        "{total} findings: {e} errors, {w} warnings, {n} notes ({parts} parts, {pads} pads)",
        "{total} hallazgos: {e} errores, {w} avisos, {n} notas ({parts} piezas, {pads} pads)",
        "{total} Befunde: {e} Fehler, {w} Warnungen, {n} Hinweise ({parts} Bauteile, {pads} Pads)",
        "{total} constats : {e} erreurs, {w} avertissements, {n} notes ({parts} composants, {pads} pastilles)",
        "{total} rilievi: {e} errori, {w} avvisi, {n} note ({parts} componenti, {pads} pad)",
        "{total} 件：エラー {e}、警告 {w}、注記 {n}（部品 {parts}、パッド {pads}）",
        "{total} 项：错误 {e}，警告 {w}，提示 {n}（{parts} 个元件，{pads} 个焊盘）",
        "{total} resultados: {e} erros, {w} avisos, {n} notas ({parts} componentes, {pads} pads)",
    ];
    t[index(lang)]
        .replace("{total}", &r.findings.len().to_string())
        .replace("{e}", &e.to_string())
        .replace("{w}", &w.to_string())
        .replace("{n}", &n.to_string())
        .replace("{parts}", &r.parts.to_string())
        .replace("{pads}", &r.pads.to_string())
}

impl Report {
    /// The findings with an error or a warning (a note does not count).
    pub fn problems(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.rule.severity() >= Severity::Warning)
            .count()
    }

    /// The report for a person, in a language.
    pub fn text(&self, lang: Lang) -> String {
        let mut out = String::new();
        for f in &self.findings {
            out.push_str(&format!(
                "[{}] {}\n",
                f.rule.severity().name(),
                message(f, lang)
            ));
        }
        if self.truncated {
            out.push_str(&format!(
                "(only the first {MAX_FINDINGS} findings are kept)\n"
            ));
        }
        out.push_str(&summary_line(lang, self));
        out.push('\n');
        out.push_str(verified_line(lang, self.verified));
        out.push('\n');
        out
    }

    /// The findings as a CSV: `severity,rule,subject,value_mm,limit_mm,x_mm,y_mm,message`.
    pub fn csv(&self, lang: Lang) -> String {
        let mut out = String::from("severity,rule,subject,value_mm,limit_mm,x_mm,y_mm,message\n");
        for f in &self.findings {
            out.push_str(&format!(
                "{},{},{},{},{},{:.4},{:.4},{}\n",
                f.rule.severity().name(),
                f.rule.name(),
                csv_field(&f.subject),
                number(f.value),
                number(f.limit),
                f.x,
                f.y,
                csv_field(&message(f, lang)),
            ));
        }
        out
    }
}
