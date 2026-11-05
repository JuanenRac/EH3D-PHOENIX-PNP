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
    /// How many children have been visited: a panel that holds itself many times over is stopped.
    visited: usize,
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
                d.children.push(Child {
                    id: id.to_string(),
                    what,
                    x: coord(k, "x", &here)?,
                    y: coord(k, "y", &here)?,
                    rotation: coord(k, "rotation", &here)?,
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
        )?;
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
            match &c.what {
                What::Panel(p) => self.walk(p, ct, &child_prefix, depth + 1, load, cache, out)?,
                What::Board(b) => {
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
