// EH3D-PHOENIX-PNP - a picture of the plan of a job, to look at before the machine moves.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The plan as an SVG picture: the feeders, the place of every part with the number of its turn and the colour of its nozzle, the way each part goes from its feeder to its place, and the
//! way the head travels over the whole job (the picks of a batch, then its places, then the next batch). The picture is one file with no script and nothing from outside, so it opens in
//! any browser or program that draws SVG, can be printed and kept with the job. The machine Y axis points up in the picture, as on the machine. Plain text: nothing is read or written here.

use crate::feeding::FeederSpec;
use crate::planner::AdvancedPlan;

/// The width of the picture, in pixels.
const WIDTH: f64 = 1000.0;
/// The tallest the picture gets, in pixels (a very long board is drawn smaller instead).
const MAX_HEIGHT: f64 = 4000.0;
const MARGIN: f64 = 48.0;
/// With more parts than this the numbers and the references are left out, because they would cover each other.
pub const MAX_LABELS: usize = 2000;
/// The colours of the nozzles, repeated for a machine with more nozzles.
const PALETTE: [&str; 6] = [
    "#d9480f", "#1971c2", "#2f9e44", "#9c36b5", "#c92a2a", "#0c8599",
];

fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // a control character cannot be in an XML text
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

fn n(v: f64) -> String {
    format!("{v:.2}")
}

fn colour(nozzle: usize) -> &'static str {
    PALETTE[nozzle % PALETTE.len()]
}

/// The picture of the plan. The feeders that are switched off are left out; a place or a feeder that is not a finite number is left out as well.
pub fn svg(plan: &AdvancedPlan, feeders: &[FeederSpec]) -> String {
    let pl = &plan.plan;
    let finite = |p: (f64, f64)| p.0.is_finite() && p.1.is_finite();
    let mut points: Vec<(f64, f64)> = Vec::new();
    for p in &pl.placements {
        points.push((p.x, p.y));
        points.push(p.pick);
    }
    let shown: Vec<&FeederSpec> = feeders
        .iter()
        .filter(|f| f.enabled && finite((f.base.x, f.base.y)))
        .collect();
    points.extend(shown.iter().map(|f| (f.base.x, f.base.y)));
    points.retain(|p| finite(*p));
    let header = |w: f64, h: f64| {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\" role=\"img\" aria-label=\"Plan of the job\">\n<title>Plan of the job</title>\n<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>\n",
            w = n(w),
            h = n(h)
        )
    };
    if points.is_empty() {
        let mut out = header(420.0, 90.0);
        out.push_str("<text x=\"20\" y=\"50\" font-family=\"sans-serif\" font-size=\"16\" fill=\"#495057\">There is nothing in the plan to draw.</text>\n</svg>\n");
        return out;
    }
    let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
    let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in &points {
        x0 = x0.min(p.0);
        x1 = x1.max(p.0);
        y0 = y0.min(p.1);
        y1 = y1.max(p.1);
    }
    let (span_x, span_y) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
    let scale = ((WIDTH - 2.0 * MARGIN) / span_x).min((MAX_HEIGHT - 2.0 * MARGIN) / span_y);
    let (width, height) = (span_x * scale + 2.0 * MARGIN, span_y * scale + 2.0 * MARGIN);
    let at = |p: (f64, f64)| (MARGIN + (p.0 - x0) * scale, MARGIN + (y1 - p.1) * scale);
    let mut out = header(width, height);

    // the way the head travels: the picks of a batch, then its places, then the next batch
    let mut route: Vec<(f64, f64)> = Vec::new();
    let mut batch = usize::MAX;
    let mut group: Vec<usize> = Vec::new();
    let flush = |group: &mut Vec<usize>, route: &mut Vec<(f64, f64)>| {
        for &i in group.iter() {
            route.push(pl.placements[i].pick);
        }
        for &i in group.iter() {
            route.push((pl.placements[i].x, pl.placements[i].y));
        }
        group.clear();
    };
    for (i, p) in pl.placements.iter().enumerate() {
        if p.batch != batch {
            flush(&mut group, &mut route);
            batch = p.batch;
        }
        group.push(i);
    }
    flush(&mut group, &mut route);
    route.retain(|p| finite(*p));
    if route.len() > 1 {
        let line: Vec<String> = route
            .iter()
            .map(|p| {
                let (x, y) = at(*p);
                format!("{},{}", n(x), n(y))
            })
            .collect();
        out.push_str(&format!(
            "<polyline id=\"travel\" fill=\"none\" stroke=\"#adb5bd\" stroke-width=\"1\" stroke-dasharray=\"4 3\" points=\"{}\"/>\n",
            line.join(" ")
        ));
    }

    // from the feeder to the place, in the colour of the nozzle
    out.push_str("<g id=\"carry\" fill=\"none\" stroke-width=\"1.4\" stroke-opacity=\"0.6\">\n");
    for p in &pl.placements {
        if finite(p.pick) && finite((p.x, p.y)) {
            let (ax, ay) = at(p.pick);
            let (bx, by) = at((p.x, p.y));
            out.push_str(&format!(
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\"/>\n",
                n(ax),
                n(ay),
                n(bx),
                n(by),
                colour(p.nozzle)
            ));
        }
    }
    out.push_str("</g>\n");

    out.push_str(
        "<g id=\"feeders\" font-family=\"sans-serif\" font-size=\"11\" fill=\"#343a40\">\n",
    );
    for f in &shown {
        let (x, y) = at((f.base.x, f.base.y));
        out.push_str(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"10\" height=\"10\" fill=\"#ced4da\" stroke=\"#495057\"><title>{} ({} / {})</title></rect>\n<text x=\"{}\" y=\"{}\">{}</text>\n",
            n(x - 5.0),
            n(y - 5.0),
            esc(&f.base.id),
            esc(&f.base.value),
            esc(&f.base.package),
            n(x + 8.0),
            n(y + 4.0),
            esc(&f.base.id)
        ));
    }
    out.push_str("</g>\n");

    let labels = pl.placements.len() <= MAX_LABELS;
    out.push_str(
        "<g id=\"places\" font-family=\"sans-serif\" font-size=\"9\" text-anchor=\"middle\">\n",
    );
    for (i, p) in pl.placements.iter().enumerate() {
        if !finite((p.x, p.y)) {
            continue;
        }
        let (x, y) = at((p.x, p.y));
        out.push_str(&format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\"><title>{}: {} from {}, nozzle {}, batch {}</title></circle>\n",
            n(x),
            n(y),
            if labels { "7" } else { "2.5" },
            colour(p.nozzle),
            esc(&p.reference),
            i + 1,
            esc(&p.feeder),
            p.nozzle + 1,
            p.batch + 1
        ));
        if labels {
            out.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" fill=\"#ffffff\">{}</text>\n",
                n(x),
                n(y + 3.0),
                i + 1
            ));
        }
    }
    out.push_str("</g>\n");

    // what the picture shows
    let nozzles = pl
        .placements
        .iter()
        .map(|p| p.nozzle + 1)
        .max()
        .unwrap_or(0);
    out.push_str(
        "<g id=\"legend\" font-family=\"sans-serif\" font-size=\"12\" fill=\"#212529\">\n",
    );
    out.push_str(&format!(
        "<text x=\"{}\" y=\"20\">{} parts in {} batches, travel {:.0} mm, about {:.0} s, {} tip changes</text>\n",
        n(MARGIN),
        pl.placements.len(),
        pl.batches,
        pl.travel_mm,
        pl.seconds,
        plan.tip_changes
    ));
    for k in 0..nozzles.min(PALETTE.len()) {
        out.push_str(&format!(
            "<circle cx=\"{}\" cy=\"34\" r=\"5\" fill=\"{}\"/><text x=\"{}\" y=\"38\">nozzle {}</text>\n",
            n(MARGIN + 6.0 + k as f64 * 80.0),
            colour(k),
            n(MARGIN + 16.0 + k as f64 * 80.0),
            k + 1
        ));
    }
    out.push_str("</g>\n</svg>\n");
    out
}
