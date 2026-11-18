//! Panels: several boards, and panels inside panels, each with a place and a turn, planned as one board. The panel is a JSON file:
//!
//! ```json
//! {
//!   "schema_version": 1,
//!   "boards": { "main": "board.pos" },
//!   "panels": {
//!     "top": { "children": [ { "id": "A", "board": "main", "x": 0, "y": 0, "rotation": 0 },
//!                            { "id": "B", "panel": "pair", "x": 60, "y": 0, "rotation": 90 } ],
//!              "fiducials": [ { "id": "f1", "x": 2, "y": 2 } ] },
//!     "pair": { "children": [ { "id": "C", "board": "main", "x": 0, "y": 0, "rotation": 0 } ] }
//!   },
//!   "root": "top"
//! }
//! ```
//!
//! A part of a board in the panel is named `child/reference` (`B/C/R1` for a board inside a panel inside the panel); the position of a child is where the origin of its board or
//! panel goes, and its rotation turns it, counter-clockwise, about that origin.
//!
//! A child with `"x_out": true` is a board (or a panel) marked as bad, an X-out: it is left out with everything inside it, and `expand_with` leaves out more by their path (`A`, `B/C`).

use crate::error::{Code, PnpError};
use crate::parts::{Part, MAX_PARTS};
use crate::units::rotate_xy;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

pub const MAX_DEPTH: usize = 8;
pub const MAX_CHILDREN: usize = 1000;
const MAX_FIDUCIALS: usize = 100;
const MAX_VISITS: usize = 100_000;
const MAX_COORDINATE: f64 = 10_000.0;

#[derive(Debug, Clone, PartialEq)]
enum What {
    Board(String),
    Panel(String),
}

#[derive(Debug, Clone, PartialEq)]
struct Child {
    id: String,
    what: What,
    x: f64,
    y: f64,
    rotation: f64,
    /// Marked as bad (X-out): the board, or everything in the panel, is not placed.
    x_out: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct PanelDef {
    children: Vec<Child>,
    fiducials: Vec<(String, f64, f64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Panel {
    boards: BTreeMap<String, String>,
    panels: BTreeMap<String, PanelDef>,
    root: String,
}

/// A child of a panel as a row of a table.
#[derive(Debug, Clone, PartialEq)]
pub struct ChildInfo {
    /// The panel the child is in.
    pub panel: String,
    pub id: String,
    /// A board (else a panel).
    pub is_board: bool,
    /// The name of the board or of the panel.
    pub target: String,
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
}

/// The parts and the fiducials of a panel, all in the coordinates of the panel.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Expanded {
    pub parts: Vec<Part>,
    /// The fiducials with their names (`id` or `child/id`) and their places.
    pub fiducials: Vec<(String, f64, f64)>,
    /// The children that were left out because they are marked as bad (X-out), by their path (`A`, `B/C`), in the order they were met.
    pub x_out: Vec<String>,
    /// How many children have been visited: a panel that holds itself many times over is stopped.
    visited: usize,
    /// The path of every child that was reached, to tell a name that no child has.
    seen: Vec<String>,
    /// How many boards were put in their places.
    boards_placed: usize,
}

fn bad(message: String) -> PnpError {
    PnpError::new(Code::PanelBad, message)
}

fn name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= 64
        && !name.contains('/')
        && !name.chars().any(char::is_control)
}

fn coord(v: &Value, key: &str, what: &str) -> Result<f64, PnpError> {
    match v.get(key) {
        None => Ok(0.0),
        Some(n) => n
            .as_f64()
            .filter(|x| x.is_finite() && x.abs() <= MAX_COORDINATE)
            .ok_or_else(|| {
                bad(format!(
                    "{what}: '{key}' is not a number up to {MAX_COORDINATE}"
                ))
            }),
    }
}

impl Panel {
    /// Reads a panel and checks it as a whole: every board and panel a child names exists, every name is usable and the numbers are numbers.
    pub fn parse(text: &str) -> Result<Panel, PnpError> {
        let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
            .map_err(|e| bad(format!("not valid JSON: {e}")))?;
        let obj = v
            .as_object()
            .ok_or_else(|| bad("the panel is not a JSON object".to_string()))?;
        if let Some(s) = obj.get("schema_version") {
            if s.as_u64() != Some(1) {
                return Err(bad(format!("schema_version {s} is not 1")));
            }
        }
        let mut boards = BTreeMap::new();
        if let Some(b) = obj.get("boards") {
            let b = b
                .as_object()
                .ok_or_else(|| bad("'boards' is not an object".to_string()))?;
            for (id, file) in b {
                let file = file
                    .as_str()
                    .filter(|f| !f.is_empty() && !f.chars().any(char::is_control))
                    .ok_or_else(|| bad(format!("the file of the board '{id}' is not a text")))?;
                if !name_ok(id) {
                    return Err(bad(format!("the board name '{id}' cannot be used")));
                }
                boards.insert(id.clone(), file.to_string());
            }
        }
        let mut panels = BTreeMap::new();
        let defs = obj
            .get("panels")
            .and_then(Value::as_object)
            .ok_or_else(|| bad("'panels' is missing or is not an object".to_string()))?;
        for (name, def) in defs {
            if !name_ok(name) {
                return Err(bad(format!("the panel name '{name}' cannot be used")));
            }
            let mut d = PanelDef::default();
            let kids = def
                .get("children")
                .and_then(Value::as_array)
                .ok_or_else(|| bad(format!("panel '{name}': 'children' is missing")))?;
            if kids.len() > MAX_CHILDREN {
                return Err(PnpError::new(
                    Code::PanelTooBig,
                    format!("panel '{name}': more than {MAX_CHILDREN} children"),
                ));
            }
            for (i, k) in kids.iter().enumerate() {
                let here = format!("panel '{name}', child {}", i + 1);
                let id = k
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|s| name_ok(s))
                    .ok_or_else(|| bad(format!("{here}: 'id' is missing or cannot be used")))?;
                if d.children.iter().any(|c| c.id == id) {
                    return Err(bad(format!("{here}: the id '{id}' is used twice")));
                }
                let what = match (
                    k.get("board").and_then(Value::as_str),
                    k.get("panel").and_then(Value::as_str),
                ) {
                    (Some(b), None) => What::Board(b.to_string()),
                    (None, Some(p)) => What::Panel(p.to_string()),
                    _ => {
                        return Err(bad(format!(
                            "{here}: it must name either a 'board' or a 'panel'"
                        )))
                    }
                };
                let x_out = match k.get("x_out") {
                    None => false,
                    Some(flag) => flag
                        .as_bool()
                        .ok_or_else(|| bad(format!("{here}: 'x_out' is not true or false")))?,
                };
                d.children.push(Child {
                    id: id.to_string(),
                    what,
                    x: coord(k, "x", &here)?,
                    y: coord(k, "y", &here)?,
                    rotation: coord(k, "rotation", &here)?,
                    x_out,
                });
            }
            if let Some(fs) = def.get("fiducials") {
                let fs = fs
                    .as_array()
                    .ok_or_else(|| bad(format!("panel '{name}': 'fiducials' is not a list")))?;
                if fs.len() > MAX_FIDUCIALS {
                    return Err(PnpError::new(
                        Code::PanelTooBig,
                        format!("panel '{name}': more than {MAX_FIDUCIALS} fiducials"),
                    ));
                }
                for (i, f) in fs.iter().enumerate() {
                    let here = format!("panel '{name}', fiducial {}", i + 1);
                    let id = f
                        .get("id")
                        .and_then(Value::as_str)
                        .filter(|s| name_ok(s))
                        .ok_or_else(|| bad(format!("{here}: 'id' is missing or cannot be used")))?;
                    d.fiducials.push((
                        id.to_string(),
                        coord(f, "x", &here)?,
                        coord(f, "y", &here)?,
                    ));
                }
            }
            panels.insert(name.clone(), d);
        }
        let root = obj
            .get("root")
            .and_then(Value::as_str)
            .ok_or_else(|| bad("'root' is missing".to_string()))?
            .to_string();
        let panel = Panel {
            boards,
            panels,
            root,
        };
        if !panel.panels.contains_key(&panel.root) {
            return Err(bad(format!(
                "the root panel '{}' does not exist",
                panel.root
            )));
        }
        for (name, d) in &panel.panels {
            for c in &d.children {
                let known = match &c.what {
                    What::Board(b) => panel.boards.contains_key(b),
                    What::Panel(p) => panel.panels.contains_key(p),
                };
                if !known {
                    return Err(bad(format!(
                        "panel '{name}', child '{}': what it names does not exist",
                        c.id
                    )));
                }
            }
        }
        Ok(panel)
    }

    /// The name of the panel the others hang from.
    pub fn root(&self) -> &str {
        &self.root
    }

    /// Every child of every panel, for a table: the panels in the order of their names and the children of each in the order of the file.
    pub fn children(&self) -> Vec<ChildInfo> {
        let mut out = Vec::new();
        for (name, def) in &self.panels {
            for c in &def.children {
                let (is_board, target) = match &c.what {
                    What::Board(b) => (true, b.clone()),
                    What::Panel(p) => (false, p.clone()),
                };
                out.push(ChildInfo {
                    panel: name.clone(),
                    id: c.id.clone(),
                    is_board,
                    target,
                    x: c.x,
                    y: c.y,
                    rotation: c.rotation,
                });
            }
        }
        out
    }

    /// How many times each board is placed in the panel once the panels inside panels are counted; a board that is never reached from the root is not in the list.
    pub fn board_counts(&self) -> BTreeMap<String, usize> {
        let mut out = BTreeMap::new();
        let mut budget = MAX_VISITS;
        self.count_boards(&self.root, 0, &mut budget, &mut out);
        out
    }

    fn count_boards(
        &self,
        name: &str,
        depth: usize,
        budget: &mut usize,
        out: &mut BTreeMap<String, usize>,
    ) {
        let Some(def) = self.panels.get(name) else {
            return;
        };
        if depth > MAX_DEPTH {
            return;
        }
        for c in &def.children {
            if *budget == 0 {
                return;
            }
            *budget -= 1;
            match &c.what {
                What::Board(b) => *out.entry(b.clone()).or_insert(0) += 1,
                What::Panel(p) => self.count_boards(p, depth + 1, budget, out),
            }
        }
    }

    /// The boards the panel uses, with their files, sorted by name.
    pub fn boards(&self) -> Vec<(String, String)> {
        self.boards
            .iter()
            .map(|(a, b)| (a.clone(), b.clone()))
            .collect()
    }

    /// Puts the boards in their places. `load` gives the parts of a board by its file; each file is loaded once, however many times the panel uses it.
    pub fn expand(
        &self,
        load: &dyn Fn(&str) -> Result<Vec<Part>, String>,
    ) -> Result<Expanded, PnpError> {
        self.expand_with(load, &[])
    }

    /// Like `expand`, with more children left out as bad (X-out) besides the ones the file marks: `x_out` names them by their path (`A`, `B/C`). A name that no child has is an
    /// error, and so is a panel in which every board is left out. A child that is left out takes with it everything inside it, the marks of a panel inside it included.
    pub fn expand_with(
        &self,
        load: &dyn Fn(&str) -> Result<Vec<Part>, String>,
        x_out: &[String],
    ) -> Result<Expanded, PnpError> {
        let mut cache: HashMap<String, Vec<Part>> = HashMap::new();
        let mut out = Expanded::default();
        self.walk(
            &self.root,
            (0.0, 0.0, 0.0),
            "",
            0,
            load,
            &mut cache,
            &mut out,
            x_out,
        )?;
        if let Some(name) = x_out.iter().find(|name| !out.seen.contains(name)) {
            return Err(PnpError::new(
                Code::PanelXOutUnknown,
                format!("'{name}' is not a board or a panel that the panel reaches"),
            ));
        }
        if !out.x_out.is_empty() && out.boards_placed == 0 {
            return Err(PnpError::new(
                Code::PanelAllBad,
                format!("{} left out and no board left", out.x_out.join(", ")),
            ));
        }
        Ok(out)
    }

    #[allow(clippy::too_many_arguments)]
    fn walk(
        &self,
        name: &str,
        t: (f64, f64, f64),
        prefix: &str,
        depth: usize,
        load: &dyn Fn(&str) -> Result<Vec<Part>, String>,
        cache: &mut HashMap<String, Vec<Part>>,
        out: &mut Expanded,
        x_out: &[String],
    ) -> Result<(), PnpError> {
        if depth > MAX_DEPTH {
            return Err(PnpError::new(
                Code::PanelTooBig,
                format!("panels are nested deeper than {MAX_DEPTH}"),
            ));
        }
        let def = self
            .panels
            .get(name)
            .ok_or_else(|| bad(format!("the panel '{name}' does not exist")))?;
        let world = |x: f64, y: f64| {
            let (rx, ry) = rotate_xy(x, y, t.2);
            (rx + t.0, ry + t.1)
        };
        for (id, fx, fy) in &def.fiducials {
            let (x, y) = world(*fx, *fy);
            out.fiducials.push((format!("{prefix}{id}"), x, y));
        }
        for c in &def.children {
            out.visited += 1;
            if out.visited > MAX_VISITS {
                return Err(PnpError::new(
                    Code::PanelTooBig,
                    format!("the panel has more than {MAX_VISITS} children in all"),
                ));
            }
            let (cx, cy) = world(c.x, c.y);
            let ct = (cx, cy, t.2 + c.rotation);
            let child_prefix = format!("{prefix}{}/", c.id);
            let path = format!("{prefix}{}", c.id);
            out.seen.push(path.clone());
            if c.x_out || x_out.contains(&path) {
                out.x_out.push(path);
                continue;
            }
            match &c.what {
                What::Panel(p) => {
                    self.walk(p, ct, &child_prefix, depth + 1, load, cache, out, x_out)?
                }
                What::Board(b) => {
                    out.boards_placed += 1;
                    let file = &self.boards[b];
                    if !cache.contains_key(file) {
                        let parts = load(file).map_err(|e| bad(format!("board '{b}': {e}")))?;
                        cache.insert(file.clone(), parts);
                    }
                    for p in &cache[file] {
                        if out.parts.len() >= MAX_PARTS {
                            return Err(PnpError::new(
                                Code::TooManyParts,
                                format!("the panel has more than {MAX_PARTS} parts"),
                            ));
                        }
                        let (rx, ry) = rotate_xy(p.x, p.y, ct.2);
                        out.parts.push(Part {
                            reference: format!("{child_prefix}{}", p.reference),
                            value: p.value.clone(),
                            package: p.package.clone(),
                            x: rx + ct.0,
                            y: ry + ct.1,
                            rotation: (p.rotation + ct.2).rem_euclid(360.0),
                            side: p.side,
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

/// A panel that is a grid of copies of one board, to be written as a panel file (`phoenix-pnp panelize`).
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    /// The file of the board, as the panel file will name it (relative to the panel file).
    pub board_file: String,
    pub cols: usize,
    pub rows: usize,
    /// The distance between the origins of two neighbours, in millimetres.
    pub pitch_x_mm: f64,
    pub pitch_y_mm: f64,
    /// The turn of every board, in degrees.
    pub rotation_deg: f64,
    /// When above 0, three fiducials are put this far outside the grid, which is taken to fill `cols * pitch_x` by `rows * pitch_y` from the origin of the first board: at the lower left
    /// corner, at the lower right one and at the upper left one. 0 puts none.
    pub fiducial_margin_mm: f64,
    /// The real size of one board along X and Y, in millimetres; above 0, the fiducials are put around the grid as it is, from the origin of the first board to the far edge of the last
    /// one (`(cols - 1) * pitch + width`), instead of the whole pitch of every board. 0 keeps the pitch (a board that fills its cell).
    pub board_width_mm: f64,
    pub board_height_mm: f64,
    /// The boards marked as bad (X-out), by the name they have in the panel (`P1`, `P2`, ... in the order of the rows, from the lower left).
    pub x_out: Vec<String>,
}

/// The most boards of a grid in each direction.
pub const MAX_GRID_SIDE: usize = 100;

/// The text of a panel file for a grid of boards: the boards are named `P1`, `P2`, ... along each row from the lower left. The text is read back with `Panel::parse` before it is given,
/// so that it is always a panel that the rest of the program reads.
pub fn grid_panel(spec: &Grid) -> Result<String, PnpError> {
    let setting = |m: String| PnpError::new(Code::PlanSetting, m);
    if spec.board_file.trim().is_empty() || spec.board_file.chars().any(char::is_control) {
        return Err(setting(
            "the file of the board is empty or has control characters".into(),
        ));
    }
    if !(1..=MAX_GRID_SIDE).contains(&spec.cols) || !(1..=MAX_GRID_SIDE).contains(&spec.rows) {
        return Err(setting(format!(
            "the grid must have 1 to {MAX_GRID_SIDE} boards in each direction"
        )));
    }
    if spec.cols * spec.rows > MAX_CHILDREN {
        return Err(setting(format!(
            "the grid has more than {MAX_CHILDREN} boards"
        )));
    }
    for (what, v, lo) in [
        ("the pitch along X", spec.pitch_x_mm, 0.001),
        ("the pitch along Y", spec.pitch_y_mm, 0.001),
    ] {
        if !v.is_finite() || !(lo..=MAX_COORDINATE).contains(&v) {
            return Err(setting(format!(
                "{what} must be from {lo} to {MAX_COORDINATE} mm"
            )));
        }
    }
    if !spec.rotation_deg.is_finite() || spec.rotation_deg.abs() > 360.0 {
        return Err(setting(
            "the turn of the boards must be from -360 to 360 degrees".into(),
        ));
    }
    if !spec.fiducial_margin_mm.is_finite() || !(0.0..=1000.0).contains(&spec.fiducial_margin_mm) {
        return Err(setting(
            "the margin of the fiducials must be from 0 to 1000 mm".into(),
        ));
    }
    for (what, v) in [
        ("the width of the board", spec.board_width_mm),
        ("the height of the board", spec.board_height_mm),
    ] {
        if !v.is_finite() || !(0.0..=MAX_COORDINATE).contains(&v) {
            return Err(setting(format!(
                "{what} must be from 0 to {MAX_COORDINATE} mm"
            )));
        }
    }
    let names: Vec<String> = (1..=spec.cols * spec.rows)
        .map(|n| format!("P{n}"))
        .collect();
    if let Some(unknown) = spec.x_out.iter().find(|x| !names.contains(x)) {
        return Err(PnpError::new(
            Code::PanelXOutUnknown,
            format!(
                "'{unknown}' is not a board of the grid (P1 to P{})",
                names.len()
            ),
        ));
    }
    let mut children = Vec::with_capacity(names.len());
    for (n, id) in names.iter().enumerate() {
        let (col, row) = (n % spec.cols, n / spec.cols);
        let mut child = serde_json::json!({
            "id": id,
            "board": "board",
            "x": col as f64 * spec.pitch_x_mm,
            "y": row as f64 * spec.pitch_y_mm,
            "rotation": spec.rotation_deg,
        });
        if spec.x_out.contains(id) {
            child["x_out"] = Value::Bool(true);
        }
        children.push(child);
    }
    let mut top = serde_json::json!({ "children": children });
    if spec.fiducial_margin_mm > 0.0 {
        // the grid fills the whole pitch of every board, unless the real size of a board is given
        let (m, w, h) = (
            spec.fiducial_margin_mm,
            if spec.board_width_mm > 0.0 {
                (spec.cols - 1) as f64 * spec.pitch_x_mm + spec.board_width_mm
            } else {
                spec.cols as f64 * spec.pitch_x_mm
            },
            if spec.board_height_mm > 0.0 {
                (spec.rows - 1) as f64 * spec.pitch_y_mm + spec.board_height_mm
            } else {
                spec.rows as f64 * spec.pitch_y_mm
            },
        );
        top["fiducials"] = serde_json::json!([
            { "id": "f1", "x": -m, "y": -m },
            { "id": "f2", "x": w + m, "y": -m },
            { "id": "f3", "x": -m, "y": h + m },
        ]);
    }
    let doc = serde_json::json!({
        "schema_version": 1,
        "boards": { "board": spec.board_file },
        "panels": { "panel": top },
        "root": "panel",
    });
    let text = serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n";
    Panel::parse(&text)?;
    Ok(text)
}
