// EH3D-PHOENIX-PNP - the geometry of the work table view.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! What the plan looks like from above, as numbers: the bodies of the parts with the mark of their polarity, the feeders, the path of the head, how a drawing is fitted to a window,
//! moved and zoomed with the pointer, and which part is under a point. Nothing here draws; the window turns these numbers into lines.

use super::workspace::{Planned, Workspace};
use crate::units::rotate_xy;

/// A place in millimetres, or a point of the screen in pixels.
pub type Point = (f64, f64);

/// The box that holds a set of points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min: Point,
    pub max: Point,
}

impl Default for Bounds {
    fn default() -> Self {
        Bounds {
            min: (f64::MAX, f64::MAX),
            max: (f64::MIN, f64::MIN),
        }
    }
}

impl Bounds {
    pub fn add(&mut self, p: Point) {
        self.min.0 = self.min.0.min(p.0);
        self.min.1 = self.min.1.min(p.1);
        self.max.0 = self.max.0.max(p.0);
        self.max.1 = self.max.1.max(p.1);
    }

    /// Whether no point was added.
    pub fn is_empty(&self) -> bool {
        self.min.0 > self.max.0 || self.min.1 > self.max.1
    }

    pub fn width(&self) -> f64 {
        if self.is_empty() {
            0.0
        } else {
            self.max.0 - self.min.0
        }
    }

    pub fn height(&self) -> f64 {
        if self.is_empty() {
            0.0
        } else {
            self.max.1 - self.min.1
        }
    }

    pub fn centre(&self) -> Point {
        if self.is_empty() {
            (0.0, 0.0)
        } else {
            (
                (self.min.0 + self.max.0) / 2.0,
                (self.min.1 + self.max.1) / 2.0,
            )
        }
    }
}

/// What the person did to the view on top of the fit: a zoom and a move.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub zoom: f64,
    pub pan: Point,
}

impl Default for Viewport {
    fn default() -> Self {
        Viewport {
            zoom: 1.0,
            pan: (0.0, 0.0),
        }
    }
}

const MIN_ZOOM: f64 = 0.1;
const MAX_ZOOM: f64 = 400.0;

impl Viewport {
    pub fn is_default(&self) -> bool {
        *self == Viewport::default()
    }

    pub fn reset(&mut self) {
        *self = Viewport::default();
    }

    /// Moves the drawing by a drag of the pointer, in pixels.
    pub fn drag(&mut self, dx: f64, dy: f64) {
        if dx.is_finite() && dy.is_finite() {
            self.pan.0 += dx;
            self.pan.1 += dy;
        }
    }

    /// Zooms with the wheel (`amount` in points of scroll) about the pointer, which is at `at` measured from the middle of the view: the point under the pointer stays under it.
    pub fn wheel(&mut self, amount: f64, at: Point) {
        if !amount.is_finite() || !at.0.is_finite() || !at.1.is_finite() {
            return;
        }
        let factor = (amount * 0.0015).exp();
        let zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let ratio = zoom / self.zoom;
        self.pan.0 = at.0 - (at.0 - self.pan.0) * ratio;
        self.pan.1 = at.1 - (at.1 - self.pan.1) * ratio;
        self.zoom = zoom;
    }
}

/// How the places in millimetres become points of the screen: measured from the middle of the view, with Y going up on the table and down on the screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    scale: f64,
    centre: Point,
    offset: Point,
}

impl Transform {
    /// Fits `bounds` into a view of `size` pixels with a margin, then applies what the person did.
    pub fn new(bounds: &Bounds, size: Point, margin: f64, view: &Viewport) -> Transform {
        let w = bounds.width().max(1.0);
        let h = bounds.height().max(1.0);
        let free_w = (size.0 - 2.0 * margin).max(1.0);
        let free_h = (size.1 - 2.0 * margin).max(1.0);
        let fit = (free_w / w).min(free_h / h);
        Transform {
            scale: fit * view.zoom,
            centre: bounds.centre(),
            offset: view.pan,
        }
    }

    /// Pixels per millimetre.
    pub fn scale(&self) -> f64 {
        self.scale
    }

    pub fn to_screen(&self, p: Point) -> Point {
        (
            self.offset.0 + (p.0 - self.centre.0) * self.scale,
            self.offset.1 - (p.1 - self.centre.1) * self.scale,
        )
    }

    pub fn to_world(&self, s: Point) -> Point {
        (
            self.centre.0 + (s.0 - self.offset.0) / self.scale,
            self.centre.1 - (s.1 - self.offset.1) / self.scale,
        )
    }
}

/// The length and the width of the body of a package in millimetres, from its name; `default_mm` for a package the program does not know.
pub fn package_size(package: &str, default_mm: f64) -> (f64, f64) {
    const CHIPS: [(&str, f64, f64); 8] = [
        ("0201", 0.6, 0.3),
        ("0402", 1.0, 0.5),
        ("0603", 1.6, 0.8),
        ("0805", 2.0, 1.25),
        ("1206", 3.2, 1.6),
        ("1210", 3.2, 2.5),
        ("1812", 4.5, 3.2),
        ("2512", 6.3, 3.2),
    ];
    let name = package.to_ascii_uppercase();
    for (code, l, w) in CHIPS {
        if name.contains(code) {
            return (l, w);
        }
    }
    let pins: Option<f64> = name
        .rsplit(|c: char| !c.is_ascii_digit())
        .find(|s| !s.is_empty())
        .and_then(|s| s.parse::<u32>().ok())
        .map(f64::from);
    let square = |n: Option<f64>| {
        let side = n.map_or(default_mm.max(3.0), |p| 1.5 + p * 0.125);
        (side, side)
    };
    if ["QFN", "DFN", "QFP", "LQFP", "TQFP", "BGA", "LGA"]
        .iter()
        .any(|k| name.contains(*k))
    {
        return square(pins);
    }
    if name.contains("SOIC") || name.contains("SOP") {
        return (5.0, 4.0);
    }
    if name.contains("SOT-23") || name.contains("SOT23") {
        return (2.9, 1.3);
    }
    if name.contains("SOD") {
        return (2.7, 1.6);
    }
    (default_mm, default_mm)
}

/// The four corners of a body of `size` (length along X before the turn) centred at `at`, turned counter-clockwise by `rotation_deg`.
pub fn outline(at: Point, size: (f64, f64), rotation_deg: f64) -> [Point; 4] {
    let (hl, hw) = (size.0 / 2.0, size.1 / 2.0);
    let corners = [(-hl, -hw), (hl, -hw), (hl, hw), (-hl, hw)];
    corners.map(|(x, y)| {
        let (rx, ry) = rotate_xy(x, y, rotation_deg);
        (at.0 + rx, at.1 + ry)
    })
}

/// The little triangle that marks the first pin of a part (the polarity): in the corner at the negative end of the length, before the turn.
pub fn polarity_mark(at: Point, size: (f64, f64), rotation_deg: f64) -> [Point; 3] {
    let (hl, hw) = (size.0 / 2.0, size.1 / 2.0);
    let t = size.0.min(size.1) * 0.45;
    let points = [(-hl, -hw), (-hl + t, -hw), (-hl, -hw + t)];
    points.map(|(x, y)| {
        let (rx, ry) = rotate_xy(x, y, rotation_deg);
        (at.0 + rx, at.1 + ry)
    })
}

/// A part of the plan as it is drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    /// The index of the placement in the plan.
    pub index: usize,
    pub reference: String,
    pub at: Point,
    pub size: (f64, f64),
    pub outline: [Point; 4],
    pub mark: [Point; 3],
    pub batch: usize,
    pub nozzle: usize,
}

/// A feeder as it is drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct FeederDot {
    pub id: String,
    pub at: Point,
    pub enabled: bool,
}

/// Everything the view of the work table draws.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    pub bounds: Bounds,
    pub shapes: Vec<Shape>,
    pub feeders: Vec<FeederDot>,
    /// The way of the head: for each batch the places where it picks, then the places where it places; the second value is the batch.
    pub path: Vec<(Point, usize)>,
}

impl Scene {
    /// The scene of a plan: the shapes at the places of the plan (mirrored for the bottom side), the feeders and the path.
    pub fn build(planned: &Planned, ws: &Workspace) -> Scene {
        let mut scene = Scene::default();
        let packages: std::collections::HashMap<&str, &str> = planned
            .parts
            .iter()
            .map(|p| (p.reference.as_str(), p.package.as_str()))
            .collect();
        let default_mm = ws.cfg.gui_part_size_mm;
        let placements = &planned.plan.plan.placements;
        for (index, p) in placements.iter().enumerate() {
            let package = packages.get(p.reference.as_str()).copied().unwrap_or("");
            let size = package_size(package, default_mm);
            let at = (p.x, p.y);
            let shape = Shape {
                index,
                reference: p.reference.clone(),
                at,
                size,
                outline: outline(at, size, p.rotation),
                mark: polarity_mark(at, size, p.rotation),
                batch: p.batch,
                nozzle: p.nozzle,
            };
            for c in shape.outline {
                scene.bounds.add(c);
            }
            scene.shapes.push(shape);
        }
        for f in &ws.feeders {
            let at = (f.base.x, f.base.y);
            scene.bounds.add(at);
            scene.feeders.push(FeederDot {
                id: f.base.id.clone(),
                at,
                enabled: f.enabled,
            });
        }
        let mut batches: Vec<usize> = placements.iter().map(|p| p.batch).collect();
        batches.sort_unstable();
        batches.dedup();
        for b in batches {
            for p in placements.iter().filter(|p| p.batch == b) {
                scene.path.push((p.pick, b));
            }
            for p in placements.iter().filter(|p| p.batch == b) {
                scene.path.push(((p.x, p.y), b));
            }
        }
        scene
    }

    /// The part nearest to a place of the table, if it is within `tolerance_mm` of the middle of the part or inside its body.
    pub fn hit(&self, at: Point, tolerance_mm: f64) -> Option<usize> {
        let mut best: Option<(f64, usize)> = None;
        for s in &self.shapes {
            let d = (s.at.0 - at.0).hypot(s.at.1 - at.1);
            let reach = tolerance_mm.max(s.size.0.max(s.size.1) / 2.0);
            if d <= reach && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, s.index));
            }
        }
        best.map(|(_, i)| i)
    }
}
