// EH3D-PHOENIX-PNP - the branch `pnp/` of an `.eh3d` project.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//
// One `.eh3d` file holds a branch for each program of the suite (the format is the crate `eh3d-project` of EH3D-COMMON, and what every program does with its branch is its module
// `appbranch`). This program writes only `pnp/`: the table of parts, the bill of materials, the table of feeders and the configuration of the machine. An import keeps the original of
// each kind of file beside a report of what was read; what is taken from another branch, such as `pcb/`, is a copy with a reference that tells when its source changes. Every other
// branch keeps its bytes. Nothing in a project is run and nothing here moves a machine.

//! The branch `pnp/` of an `.eh3d` project.

use std::path::Path;

use eh3d_project::appbranch::{
    self, BranchError, BranchKit, BranchResult, Conversion, Notice, WorkingCopy,
};
use eh3d_project::meta::{ConversionReport, ORIGIN_DIR, REPORT_FILE};

pub use eh3d_project::appbranch::{Identity, Listing, Row};

use crate::bom::Bom;
use crate::config::{MachineConfig, MAX_CONFIG_BYTES};
use crate::error::{Code, Lang, PnpError};
use crate::feeding;
use crate::import::{self, ColumnMap, Format};
use crate::parts::{decode_text, parse_all};

/// The branch this program owns.
pub const BRANCH: &str = "pnp";
/// The format of the branch, `major.minor`.
pub const FORMAT: &str = "1.0";
/// The record of the branch that says which of its files is what.
pub const MANIFEST: &str = "pnp.json";

const CONFIG_FILE: &str = "settings/machine.cfg";

const KIT: BranchKit<'static> = BranchKit::new(BRANCH, FORMAT, MANIFEST, env!("CARGO_PKG_VERSION"));

/// The kinds of table of the work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A position (centroid) file.
    Parts,
    /// A bill of materials.
    Bom,
    /// A table of feeders.
    Feeders,
}

impl Kind {
    /// The three kinds.
    pub const ALL: [Kind; 3] = [Kind::Parts, Kind::Bom, Kind::Feeders];

    /// The word of the command line, the key of the record and the folder of the branch.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Parts => "parts",
            Kind::Bom => "bom",
            Kind::Feeders => "feeders",
        }
    }

    /// The kind a word names.
    pub fn parse(text: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == text)
    }

    fn folder(self) -> String {
        format!("{}/", self.name())
    }

    fn origin(self) -> String {
        format!("{ORIGIN_DIR}{}/", self.name())
    }
}

/// What `save` is given: only what is there replaces what the branch holds.
#[derive(Debug, Default, Clone)]
pub struct Inputs {
    /// The table of parts (a KiCad `.pos` file or a CSV).
    pub parts: Option<String>,
    /// The bill of materials.
    pub bom: Option<String>,
    /// The table of feeders.
    pub feeders: Option<String>,
    /// The configuration of the machine.
    pub config: Option<String>,
}

/// How a position file of another program is read.
#[derive(Debug, Clone)]
pub struct PositionOptions {
    /// The format; `Format::Auto` tells it from the text.
    pub format: Format,
    /// The columns of a CSV that the program does not know.
    pub map: Option<ColumnMap>,
}

impl Default for PositionOptions {
    fn default() -> PositionOptions {
        PositionOptions {
            format: Format::Auto,
            map: None,
        }
    }
}

/// What the branch `pnp/` holds.
#[derive(Debug, Clone, Default)]
pub struct Opened {
    /// The table of parts, as a name inside the branch.
    pub parts: Option<String>,
    /// How many parts are to be mounted and how many are not.
    pub part_count: Option<(usize, usize)>,
    /// The bill of materials.
    pub bom: Option<String>,
    /// How many rows and references it has.
    pub bom_count: Option<(usize, usize)>,
    /// The table of feeders.
    pub feeders: Option<String>,
    /// How many feeders it has.
    pub feeder_count: Option<usize>,
    /// The configuration of the machine.
    pub config: Option<String>,
    /// The reports of the last import of each kind.
    pub reports: Vec<(Kind, ConversionReport)>,
    /// What is out of date or wrong.
    pub warnings: Vec<String>,
    /// The files written out, when the branch was taken out.
    pub written: Option<usize>,
}

fn describe(e: BranchError) -> String {
    e.to_string()
}

fn said(notices: Vec<Notice>) -> Vec<String> {
    notices.iter().map(ToString::to_string).collect()
}

/// The text of a file of the program, read as the commands read it (UTF-8, UTF-16 with its mark or Latin-1).
fn text_of(bytes: &[u8]) -> String {
    decode_text(bytes.to_vec()).0
}

fn pnp_text(name: &str, e: PnpError) -> String {
    format!("{name}: {}", e.localized(Lang::En))
}

fn too_big(name: &str, size: usize, limit: u64) -> Result<(), String> {
    if size as u64 > limit {
        return Err(pnp_text(
            name,
            PnpError::new(
                Code::FileTooBig,
                format!("it has {size} bytes, the limit is {limit}"),
            ),
        ));
    }
    Ok(())
}

/// Reads a table of a kind with the reader of the program and says how much there is in it.
fn count(
    kind: Kind,
    name: &str,
    bytes: &[u8],
    cfg: &MachineConfig,
) -> Result<(usize, usize), String> {
    too_big(name, bytes.len(), cfg.max_file_bytes)?;
    let text = text_of(bytes);
    match kind {
        Kind::Parts => parse_all(&text, &cfg.profile)
            .map(|p| (p.parts.len(), p.dnp.len()))
            .map_err(|e| format!("{name}: {e}")),
        Kind::Bom => Bom::parse(&text, cfg.max_parts)
            .map(|b| (b.lines.len(), b.reference_count()))
            .map_err(|e| pnp_text(name, e)),
        Kind::Feeders => feeding::from_csv_with(&text, cfg.tray_fill)
            .map(|f| (f.len(), 0))
            .map_err(|e| pnp_text(name, e)),
    }
}

/// A file read for the project: its name and its bytes, never more than `limit`.
fn read_file(file: &Path, limit: u64) -> Result<(String, Vec<u8>), String> {
    let name = appbranch::file_name(file).map_err(describe)?;
    let bytes = appbranch::read_bounded(file, limit).map_err(describe)?;
    Ok((name, bytes))
}

/// Puts the work of the assembly into the branch `pnp/` of the project at `path`, creating the project when it is not there. Every file is read with the reader of the program before
/// anything is written; what is not given stays as it was.
pub fn save(
    path: &Path,
    inputs: &Inputs,
    identity: &Identity,
    cfg: &MachineConfig,
) -> Result<Vec<String>, String> {
    let given = [
        (Kind::Parts, &inputs.parts),
        (Kind::Bom, &inputs.bom),
        (Kind::Feeders, &inputs.feeders),
    ];
    if given.iter().all(|(_, f)| f.is_none()) && inputs.config.is_none() {
        return Err(
            "there is nothing to save: give parts, a bill of materials, feeders or a configuration"
                .into(),
        );
    }
    let config = match &inputs.config {
        Some(file) => {
            let (name, bytes) = read_file(Path::new(file), MAX_CONFIG_BYTES as u64)?;
            MachineConfig::parse(&text_of(&bytes)).map_err(|e| pnp_text(&name, e))?;
            Some(bytes)
        }
        None => None,
    };
    let mut tables = Vec::new();
    for (kind, file) in given {
        let Some(file) = file else { continue };
        let (name, bytes) = read_file(Path::new(file), cfg.max_file_bytes)?;
        count(kind, &name, &bytes, cfg)?;
        let rel = format!("{}{name}", kind.folder());
        KIT.check_name(&rel).map_err(describe)?;
        tables.push((kind, rel, bytes));
    }

    let (_, notices) = KIT
        .update(path, identity, |_, copy| {
            for (kind, rel, bytes) in tables {
                copy.put(&rel, bytes)?;
                copy.choose(kind.name(), &kind.folder(), &rel);
            }
            if let Some(bytes) = config {
                copy.put(CONFIG_FILE, bytes)?;
                copy.set("config", CONFIG_FILE);
            }
            Ok(())
        })
        .map_err(describe)?;
    Ok(said(notices))
}

/// What an import of one file gives: what was read and the file that the work uses (its name inside the branch and its bytes).
struct Converted {
    format: String,
    conversion: Conversion,
    work: (String, Vec<u8>),
}

fn convert(
    kind: Kind,
    name: &str,
    bytes: &[u8],
    cfg: &MachineConfig,
    positions: &PositionOptions,
) -> Result<Converted, String> {
    too_big(name, bytes.len(), cfg.max_file_bytes)?;
    let mut conversion = Conversion::default();
    let stem = Path::new(name).file_stem().map_or_else(
        || kind.name().to_string(),
        |s| s.to_string_lossy().into_owned(),
    );
    match kind {
        Kind::Parts => {
            let imported = import::import(
                &text_of(bytes),
                positions.format,
                positions.map.as_ref(),
                &cfg.import_settings(),
            )
            .map_err(|e| pnp_text(name, e))?;
            let dnp = imported.dnp_parts().len();
            conversion
                .read
                .push(appbranch::counted("parts", imported.rows.len(), None));
            conversion
                .read
                .push(appbranch::counted("parts not to be mounted", dnp, None));
            conversion.export_losses.push(appbranch::counted(
                "columns",
                1,
                Some("the table of parts holds reference, value, package, place, turn, side and mounting in millimetres; anything else is only in the original"),
            ));
            let table = imported.to_csv().into_bytes();
            Ok(Converted {
                format: imported.format.name().to_string(),
                conversion,
                work: (format!("{}{stem}.csv", kind.folder()), table),
            })
        }
        Kind::Bom => {
            let (rows, references) = count(kind, name, bytes, cfg)?;
            conversion.read.push(appbranch::counted("rows", rows, None));
            conversion
                .read
                .push(appbranch::counted("references", references, None));
            Ok(Converted {
                format: "bom-csv".to_string(),
                conversion,
                work: (format!("{}{name}", kind.folder()), bytes.to_vec()),
            })
        }
        Kind::Feeders => {
            let (feeders, _) = count(kind, name, bytes, cfg)?;
            conversion
                .read
                .push(appbranch::counted("feeders", feeders, None));
            Ok(Converted {
                format: "feeders-csv".to_string(),
                conversion,
                work: (format!("{}{name}", kind.folder()), bytes.to_vec()),
            })
        }
    }
}

/// Imports a file of another program. The original is kept byte for byte under `pnp/origin/KIND/` with its report, and `pnp/origin/report.json` is the report of the last import. An
/// import replaces the import of the same kind that was there; the originals of the other kinds stay. A position file becomes a table of parts of the program; a bill of materials and
/// a table of feeders are used as they are.
pub fn import_file(
    path: &Path,
    source: &Path,
    kind: Kind,
    identity: &Identity,
    cfg: &MachineConfig,
    positions: &PositionOptions,
) -> Result<(ConversionReport, Vec<String>), String> {
    if kind != Kind::Parts && (positions.map.is_some() || positions.format != Format::Auto) {
        return Err("a format and a column map are for a position file".into());
    }
    let (name, bytes) = read_file(source, cfg.max_file_bytes)?;
    let converted = convert(kind, &name, &bytes, cfg, positions)?;
    let folder = kind.origin();
    let original = format!("{folder}{name}");
    let report_of_kind = format!("{folder}report.json");
    if original == report_of_kind {
        return Err(format!(
            "{name}: the file has the name of the report; rename it"
        ));
    }
    let mut report = appbranch::conversion_report(
        "phoenix-pnp",
        env!("CARGO_PKG_VERSION"),
        &converted.format,
        &name,
        &bytes,
        converted.conversion,
    );
    report.original = Some(original.clone());
    let mut text = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
    text.push(b'\n');
    let (rel, work) = converted.work;
    KIT.check_name(&original).map_err(describe)?;
    KIT.check_name(&rel).map_err(describe)?;

    let mut replaced = false;
    let (_, notices) = KIT
        .update(path, identity, |_, copy| {
            let before: Vec<String> = copy
                .files()
                .keys()
                .filter(|n| n.starts_with(&folder))
                .cloned()
                .collect();
            replaced = before
                .iter()
                .any(|n| *n != report_of_kind && *n != original);
            for old in &before {
                copy.remove(old);
            }
            copy.put(&original, bytes)?;
            copy.put(&report_of_kind, text.clone())?;
            copy.put(REPORT_FILE, text)?;
            copy.put(&rel, work)?;
            copy.choose(kind.name(), &kind.folder(), &rel);
            Ok(())
        })
        .map_err(describe)?;
    let mut notes = said(notices);
    if replaced {
        notes.push(Notice::ImportReplaced.to_string());
    }
    Ok((report, notes))
}

/// What a file of another branch is for an assembly: the kind of table, when the program can read it as one.
fn kind_of(rel: &str, bytes: &[u8], cfg: &MachineConfig) -> Option<Kind> {
    let extension = appbranch::extension_of(rel);
    if !matches!(extension.as_str(), "csv" | "pos" | "txt" | "tsv") {
        return None;
    }
    // the table of feeders has its own header, a position file has places and a bill of materials has only references: tried in that order
    [Kind::Feeders, Kind::Parts, Kind::Bom]
        .into_iter()
        .find(|&kind| count(kind, rel, bytes, cfg).is_ok_and(|(n, _)| n > 0))
}

/// What [`take`] gives: the name of the copy inside the branch, the table of the work it became and what a person should be told.
pub type Taken = (String, Option<Kind>, Vec<String>);

/// Copies a file of another branch, such as `pcb/`, into `pnp/taken/`, with a reference that remembers the state of its source. A table of positions, a bill of materials or a table of
/// feeders becomes the one of the work; any other file is only kept. `kind` says which of the three it must be, and a file that cannot be read as that is refused.
pub fn take(
    path: &Path,
    branch: &str,
    rel: &str,
    kind: Option<Kind>,
    cfg: &MachineConfig,
) -> Result<Taken, String> {
    if branch == BRANCH {
        return Err("a branch takes from another branch, not from itself".to_string());
    }
    // read first, to know what it is before anything is written
    let bytes = appbranch::open_project(path)
        .map_err(describe)?
        .read_entry(branch, rel)
        .map_err(|e| e.to_string())?;
    too_big(&format!("{branch}/{rel}"), bytes.len(), cfg.max_file_bytes)?;
    let found = match kind {
        Some(k) => {
            count(k, &format!("{branch}/{rel}"), &bytes, cfg)?;
            Some(k)
        }
        None => kind_of(rel, &bytes, cfg),
    };
    let artifact = match found {
        Some(Kind::Parts) => "placements",
        Some(Kind::Bom) => "bom",
        Some(Kind::Feeders) => "feeders",
        None => "file",
    };
    let (target, notices) = KIT
        .take(
            path,
            branch,
            rel,
            artifact,
            |now: &[u8]| -> BranchResult<()> {
                if now == bytes.as_slice() {
                    Ok(())
                } else {
                    Err(BranchError::Refused(format!(
                        "{branch}/{rel}: the file changed while it was being taken"
                    )))
                }
            },
            |copy: &mut WorkingCopy, target: &str| {
                if let Some(k) = found {
                    copy.choose(k.name(), &k.folder(), target);
                }
            },
        )
        .map_err(describe)?;
    Ok((target, found, said(notices)))
}

/// The branches of a project, with what is out of date.
pub fn list(path: &Path) -> Result<Listing, String> {
    appbranch::list(path).map_err(describe)
}

/// Opens the branch `pnp/`: reads and checks what it holds with the readers of the program (with the configuration kept in the branch, else `cfg`) and, with `into`, writes its files
/// there (a file that is there is never written over).
pub fn open(path: &Path, into: Option<&Path>, cfg: &MachineConfig) -> Result<Opened, String> {
    let (mut project, copy, warnings) = KIT.open(path).map_err(describe)?;
    let mut opened = Opened {
        parts: copy.text(Kind::Parts.name()),
        bom: copy.text(Kind::Bom.name()),
        feeders: copy.text(Kind::Feeders.name()),
        config: copy.text("config"),
        warnings,
        ..Opened::default()
    };
    let missing = |rel: &str| format!("{BRANCH}/{rel}: the file is not in the branch");
    let kept = match &opened.config {
        Some(rel) => {
            let bytes = copy.file(rel).ok_or_else(|| missing(rel))?;
            Some(MachineConfig::parse(&text_of(bytes)).map_err(|e| pnp_text(rel, e))?)
        }
        None => None,
    };
    let cfg = kept.as_ref().unwrap_or(cfg);
    let read = |kind: Kind, rel: &Option<String>| -> Result<Option<(usize, usize)>, String> {
        let Some(rel) = rel else { return Ok(None) };
        let bytes = copy.file(rel).ok_or_else(|| missing(rel))?;
        count(kind, rel, bytes, cfg).map(Some)
    };
    opened.part_count = read(Kind::Parts, &opened.parts)?;
    opened.bom_count = read(Kind::Bom, &opened.bom)?;
    opened.feeder_count = read(Kind::Feeders, &opened.feeders)?.map(|(n, _)| n);
    for kind in Kind::ALL {
        let rel = format!("{}report.json", kind.origin());
        if let Some(bytes) = copy.file(&rel) {
            let report = serde_json::from_slice(bytes)
                .map_err(|e| format!("{BRANCH}/{rel} is not a valid record: {e}"))?;
            opened.reports.push((kind, report));
        }
    }
    if let Some(dir) = into {
        opened.written = Some(
            project
                .extract_branch(BRANCH, dir)
                .map_err(|e| e.to_string())?,
        );
    }
    Ok(opened)
}

/// Writes the files of any branch into a folder, byte for byte (a file that is there is never written over).
pub fn extract(path: &Path, branch: &str, dir: &Path) -> Result<usize, String> {
    appbranch::extract(path, branch, dir).map_err(describe)
}
