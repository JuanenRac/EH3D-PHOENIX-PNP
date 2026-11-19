// EH3D-PHOENIX-PNP - the size of the body of a package, read from a footprint file of KiCad.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A footprint of KiCad (`.kicad_mod`) is a text of nested lists. The drawing of the body is on the fabrication layer (`F.Fab`, or `B.Fab` for a footprint of the bottom side), and the box that
//! holds that drawing is the body of the part: its long side and its short side are `body_long_mm` and `body_short_mm` of a table of packages. A footprint with no drawing on that layer falls
//! back to the courtyard (`F.CrtYd`, `B.CrtYd`), which is bigger than the body, and says so.
//!
//! The reading is a safe one: a file is read as text, never run; it has a size limit, the lists may be nested only so deep and hold only so many items, a number has to be finite and within
//! 10000 millimetres, and a file that is not understood is an error with its reason (and the code `PHOENIX-PNP-010`) and never a guess. Nothing here writes anything. The old format
//! (`(module ...)`) is read as well. The height of the part is not in the footprint: it comes from the package when its name is a known one (`pkggen`), or from the person.

use crate::error::{Code, PnpError};
use crate::pkggen::{self, PackageSpec};
use crate::units::rotate_xy;

/// The largest file that is read, in bytes.
pub const MAX_BYTES: usize = 1_048_576;
/// How deep the lists may be nested.
const MAX_DEPTH: usize = 48;
/// The most items (lists and words) in a file.
const MAX_NODES: usize = 200_000;
/// The largest size of a coordinate, in millimetres.
const MAX_COORD: f64 = 10_000.0;
/// The smallest and the largest side of a body, in millimetres.
const MIN_SIDE: f64 = 0.05;
const MAX_SIDE: f64 = 200.0;

fn bad(message: impl Into<String>) -> PnpError {
    PnpError::new(Code::CatalogBadRow, message)
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Word(String),
    Text(String),
    List(Vec<Node>),
}

impl Node {
    fn text(&self) -> Option<&str> {
        match self {
            Node::Word(s) | Node::Text(s) => Some(s),
            Node::List(_) => None,
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    nodes: usize,
}

impl Reader<'_> {
    fn blank(&mut self) {
        while self.at < self.bytes.len() && self.bytes[self.at].is_ascii_whitespace() {
            self.at += 1;
        }
    }

    fn count(&mut self) -> Result<(), PnpError> {
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err(bad(format!(
                "the footprint has more than {MAX_NODES} items"
            )));
        }
        Ok(())
    }

    /// A list; `self.at` is on its opening bracket.
    fn list(&mut self, depth: usize) -> Result<Node, PnpError> {
        if depth > MAX_DEPTH {
            return Err(bad(format!(
                "the footprint nests lists deeper than {MAX_DEPTH} levels"
            )));
        }
        self.count()?;
        self.at += 1;
        let mut items: Vec<Node> = Vec::new();
        loop {
            self.blank();
            match self.bytes.get(self.at) {
                None => {
                    return Err(bad(
                        "the footprint ends inside a list: a bracket is not closed",
                    ))
                }
                Some(b')') => {
                    self.at += 1;
                    return Ok(Node::List(items));
                }
                Some(b'(') => items.push(self.list(depth + 1)?),
                Some(b'"') => items.push(self.quoted()?),
                Some(_) => items.push(self.word()?),
            }
        }
    }

    fn quoted(&mut self) -> Result<Node, PnpError> {
        self.count()?;
        self.at += 1;
        let mut out: Vec<u8> = Vec::new();
        while let Some(&b) = self.bytes.get(self.at) {
            self.at += 1;
            match b {
                b'"' => return Ok(Node::Text(String::from_utf8_lossy(&out).into_owned())),
                b'\\' => {
                    if let Some(&n) = self.bytes.get(self.at) {
                        self.at += 1;
                        out.push(n);
                    }
                }
                _ => out.push(b),
            }
        }
        Err(bad("the footprint ends inside a quoted text"))
    }

    fn word(&mut self) -> Result<Node, PnpError> {
        self.count()?;
        let start = self.at;
        while let Some(&b) = self.bytes.get(self.at) {
            if b.is_ascii_whitespace() || matches!(b, b'(' | b')' | b'"') {
                break;
            }
            self.at += 1;
        }
        Ok(Node::Word(
            String::from_utf8_lossy(&self.bytes[start..self.at]).into_owned(),
        ))
    }
}

fn parse_tree(text: &str) -> Result<Vec<Node>, PnpError> {
    let bytes = text.trim_start_matches('\u{feff}').as_bytes();
    if bytes.len() > MAX_BYTES {
        return Err(bad(format!(
            "the footprint has {} bytes, the limit is {MAX_BYTES}",
            bytes.len()
        )));
    }
    let mut r = Reader {
        bytes,
        at: 0,
        nodes: 0,
    };
    r.blank();
    if r.bytes.get(r.at) != Some(&b'(') {
        return Err(bad(
            "this is not a footprint of KiCad: the text does not start with a bracket",
        ));
    }
    let Node::List(root) = r.list(0)? else {
        return Err(bad("the footprint is empty"));
    };
    r.blank();
    if r.at < r.bytes.len() {
        return Err(bad("there is text after the end of the footprint"));
    }
    Ok(root)
}

fn number(node: Option<&Node>, what: &str) -> Result<f64, PnpError> {
    let t = node
        .and_then(Node::text)
        .ok_or_else(|| bad(format!("{what}: a number is missing")))?;
    t.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && v.abs() <= MAX_COORD)
        .ok_or_else(|| bad(format!("{what}: '{t}' is not a usable number")))
}

/// The first item of a list that is itself a list called `key`: its items after the name.
fn child<'a>(items: &'a [Node], key: &str) -> Option<&'a [Node]> {
    items.iter().find_map(|n| match n {
        Node::List(l) if l.first().and_then(Node::text) == Some(key) => Some(&l[1..]),
        _ => None,
    })
}

fn point(items: &[Node], key: &str) -> Result<Option<(f64, f64)>, PnpError> {
    match child(items, key) {
        None => Ok(None),
        Some(c) => Ok(Some((number(c.first(), key)?, number(c.get(1), key)?))),
    }
}

/// The box that holds a drawing.
#[derive(Debug, Clone, Copy, Default)]
struct Bounds {
    min: (f64, f64),
    max: (f64, f64),
    any: bool,
}

impl Bounds {
    fn add(&mut self, p: (f64, f64)) {
        if self.any {
            self.min = (self.min.0.min(p.0), self.min.1.min(p.1));
            self.max = (self.max.0.max(p.0), self.max.1.max(p.1));
        } else {
            self.min = p;
            self.max = p;
            self.any = true;
        }
    }

    fn sides(&self) -> (f64, f64) {
        (self.max.0 - self.min.0, self.max.1 - self.min.1)
    }
}

/// Where the size of the body came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodySource {
    /// The drawing of the body on the fabrication layer.
    Fab,
    /// The courtyard, which is bigger than the body.
    Courtyard,
}

/// What was read from a footprint.
#[derive(Debug, Clone, PartialEq)]
pub struct Footprint {
    /// The name of the footprint, without the name of a library in front.
    pub name: String,
    pub body_long_mm: f64,
    pub body_short_mm: f64,
    pub source: BodySource,
    /// How many pads it has.
    pub pads: usize,
}

fn layer_of(items: &[Node]) -> Option<&str> {
    child(items, "layer")
        .and_then(|l| l.first())
        .and_then(Node::text)
}

fn add_points(items: &[Node], keys: &[&str], b: &mut Bounds) -> Result<(), PnpError> {
    for k in keys {
        if let Some(p) = point(items, k)? {
            b.add(p);
        }
    }
    Ok(())
}

/// Puts the drawing of one graphic item (a line, a rectangle, a circle, an arc, a polygon or a curve) in the box.
fn add_graphic(kind: &str, items: &[Node], b: &mut Bounds) -> Result<(), PnpError> {
    match kind {
        "fp_line" | "fp_rect" => add_points(items, &["start", "end"], b),
        "fp_circle" => {
            let c = point(items, "center")?.ok_or_else(|| bad("a circle has no center"))?;
            let e = point(items, "end")?.ok_or_else(|| bad("a circle has no end"))?;
            let r = (e.0 - c.0).hypot(e.1 - c.1);
            b.add((c.0 - r, c.1 - r));
            b.add((c.0 + r, c.1 + r));
            Ok(())
        }
        "fp_arc" => {
            if let Some(angle) = child(items, "angle") {
                // the old form: the center, the point where the arc starts and the angle it turns
                let c = point(items, "start")?.ok_or_else(|| bad("an arc has no center"))?;
                let e = point(items, "end")?.ok_or_else(|| bad("an arc has no start"))?;
                let a = number(angle.first(), "angle")?;
                let steps = a.abs().ceil().clamp(1.0, 360.0) as usize;
                for i in 0..=steps {
                    let t = a * (i as f64) / (steps as f64);
                    let r = rotate_xy(e.0 - c.0, e.1 - c.1, t);
                    b.add((c.0 + r.0, c.1 + r.1));
                }
                Ok(())
            } else {
                // the form with the middle point: the three points hold the arc to a close approximation
                add_points(items, &["start", "mid", "end"], b)
            }
        }
        "fp_poly" | "fp_curve" => {
            if let Some(pts) = child(items, "pts") {
                for n in pts {
                    if let Node::List(l) = n {
                        if l.first().and_then(Node::text) == Some("xy") {
                            b.add((number(l.get(1), "xy")?, number(l.get(2), "xy")?));
                        }
                    }
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Reads a footprint of KiCad (the text of a `.kicad_mod` file): its name, the box of the drawing of its body and how many pads it has.
pub fn parse_footprint(text: &str) -> Result<Footprint, PnpError> {
    let root = parse_tree(text)?;
    match root.first().and_then(Node::text) {
        Some("footprint" | "module") => {}
        _ => {
            return Err(bad(
                "this is not a footprint of KiCad: the first word is not 'footprint' or 'module'",
            ))
        }
    }
    let raw = root
        .get(1)
        .and_then(Node::text)
        .filter(|n| !n.trim().is_empty())
        .ok_or_else(|| bad("the footprint has no name"))?;
    let name = raw.rsplit(':').next().unwrap_or(raw).trim().to_string();
    if name.is_empty() || name.chars().any(char::is_control) {
        return Err(bad(
            "the name of the footprint is empty or has a control character",
        ));
    }
    let (mut fab, mut yard) = (Bounds::default(), Bounds::default());
    let mut pads = 0usize;
    for n in &root[2..] {
        let Node::List(l) = n else { continue };
        let Some(kind) = l.first().and_then(Node::text) else {
            continue;
        };
        if kind == "pad" {
            pads += 1;
            continue;
        }
        if !kind.starts_with("fp_") {
            continue;
        }
        match layer_of(l) {
            Some("F.Fab" | "B.Fab") => add_graphic(kind, &l[1..], &mut fab)?,
            Some("F.CrtYd" | "B.CrtYd") => add_graphic(kind, &l[1..], &mut yard)?,
            _ => {}
        }
    }
    let (b, source) = if fab.any {
        (fab, BodySource::Fab)
    } else if yard.any {
        (yard, BodySource::Courtyard)
    } else {
        return Err(bad(format!(
            "'{name}' has no drawing on the fabrication layer (F.Fab) or on the courtyard (F.CrtYd): the size of the body cannot be told"
        )));
    };
    let (w, h) = b.sides();
    let (long, short) = (w.max(h), w.min(h));
    if short < MIN_SIDE || long > MAX_SIDE {
        return Err(bad(format!(
            "'{name}': the drawing of the body is {w:.3} by {h:.3} mm, outside {MIN_SIDE} to {MAX_SIDE} mm: it is not the outline of a body"
        )));
    }
    Ok(Footprint {
        name,
        body_long_mm: round3(long),
        body_short_mm: round3(short),
        source,
        pads,
    })
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// The height of a part that has the name of a footprint, when the name is that of a package the program knows (`R_0603_1608Metric` is a chip of `0603`) and its body is within a quarter
/// of the one of the drawing. `None` when the name says nothing, which is not a reason to guess.
pub fn height_hint(fp: &Footprint) -> Option<f64> {
    let parts: Vec<&str> = fp.name.split('_').collect();
    let mut tries: Vec<String> = vec![fp.name.clone(), parts[0].to_string()];
    if parts.len() >= 2 {
        tries.push(format!("{}_{}", parts[0], parts[1]));
    }
    let close = |a: f64, b: f64| (a - b).abs() <= 0.25 * b;
    tries.iter().find_map(|t| {
        let s = pkggen::generate(t).ok()?;
        (close(s.body_long_mm, fp.body_long_mm) && close(s.body_short_mm, fp.body_short_mm))
            .then_some(s.height_mm)
    })
}

/// The row of a table of packages for a footprint: the body of the drawing, the width of the tape that holds such a body and the height that `height` gives (or the package, when the
/// name is a known one). Without either, an error that says to give the height.
pub fn package_spec(fp: &Footprint, height: Option<f64>) -> Result<PackageSpec, PnpError> {
    let height_mm = match height {
        Some(h) if h.is_finite() && (0.01..=50.0).contains(&h) => h,
        Some(h) => {
            return Err(bad(format!(
                "the height {h} mm is outside 0.01 to 50 mm"
            )))
        }
        None => height_hint(fp).ok_or_else(|| {
            bad(format!(
                "'{}': the height of the part is not in the footprint and the name is not a known package: give it with --height MM",
                fp.name
            ))
        })?,
    };
    Ok(PackageSpec {
        name: fp.name.clone(),
        height_mm,
        tape_mm: pkggen::tape_for(fp.body_long_mm, fp.body_short_mm),
        body_long_mm: fp.body_long_mm,
        body_short_mm: fp.body_short_mm,
    })
}

impl Footprint {
    /// A warning about where the size came from, or `None` when it is the drawing of the body.
    pub fn note(&self) -> Option<String> {
        match self.source {
            BodySource::Fab => None,
            BodySource::Courtyard => Some(format!(
                "'{}' has no drawing of the body on the fabrication layer: the size is that of the courtyard, which is bigger than the body",
                self.name
            )),
        }
    }
}
