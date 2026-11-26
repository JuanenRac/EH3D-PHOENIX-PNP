//! The branch `pnp/` of an `.eh3d` project: what is saved comes back byte for byte, every other branch (known or not) keeps its bytes, a copy taken from `pcb/` is told as out of date
//! when its source changes, an imported file of each kind is kept with its report, and a damaged file is an error and never a crash. Every project here is made up.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use eh3d_container::{sha256_hex, AtomicOptions};
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::eh3dfile::{self, Identity, Inputs, Kind, PositionOptions};
use eh3d_phoenix_pnp::parts::parse_all;
use eh3d_project::{BranchUpdate, NewProject, OpenOptions, Project, SaveOptions, Seal, Writer};

fn example(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

/// A folder of its own for one test.
fn folder(tag: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("eh3dfile_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn cfg() -> MachineConfig {
    MachineConfig::default()
}

fn write_branch(path: &Path, branch: &str, entries: &[(&str, &[u8])]) {
    let project = Project::open(path, &OpenOptions::default()).unwrap();
    let mut update = BranchUpdate::new(branch, "1.0", "0.1.0");
    for (rel, bytes) in entries {
        update = update.with_entry(rel, bytes);
    }
    project
        .save_branch(
            &Writer::new(branch, &[branch]),
            update,
            &SaveOptions::default(),
        )
        .unwrap();
}

/// A project with a board, branches of other programs, one this program does not know and an extension folder.
fn synthetic(path: &Path) {
    let new = NewProject {
        id: "00000000-0000-4000-8000-000000000003".into(),
        name: "Synthetic".into(),
        units: "millimetres".into(),
        created: None,
    };
    Project::create(
        path,
        new,
        &OpenOptions::default(),
        &AtomicOptions::default(),
    )
    .unwrap();
    let pos = fs::read(example("board.pos")).unwrap();
    let bom = fs::read(example("bom.csv")).unwrap();
    write_branch(
        path,
        "pcb",
        &[
            ("out/board.pos", pos.as_slice()),
            ("out/bom.csv", bom.as_slice()),
            ("board.bin", &[1u8, 2, 3][..]),
        ],
    );
    write_branch(path, "holography", &[("data.bin", &[0u8, 255, 7, 7, 7])]);
    write_branch(path, "x-vendor", &[("blob", b"anything at all")]);
}

/// Every file of every branch but `pnp`, the records included.
fn others(path: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut project = Project::open(path, &OpenOptions::default()).unwrap();
    let names: Vec<(String, String)> = project
        .branches()
        .filter(|b| b.id != "pnp")
        .flat_map(|b| b.files.keys().map(|f| (b.id.clone(), f.clone())))
        .collect();
    names
        .into_iter()
        .map(|(b, f)| (format!("{b}/{f}"), project.read_entry(&b, &f).unwrap()))
        .collect()
}

fn work() -> Inputs {
    Inputs {
        parts: Some(example("board.pos")),
        bom: Some(example("bom.csv")),
        feeders: Some(example("feeders.csv")),
        config: Some(example("machine.cfg")),
    }
}

fn import(
    file: &Path,
    source: &str,
    kind: Kind,
) -> Result<(eh3d_project::meta::ConversionReport, Vec<String>), String> {
    eh3dfile::import_file(
        file,
        Path::new(source),
        kind,
        &Identity::default(),
        &cfg(),
        &PositionOptions::default(),
    )
}

fn files(path: &Path) -> Vec<String> {
    let project = Project::open(path, &OpenOptions::default()).unwrap();
    project
        .branch("pnp")
        .unwrap()
        .files
        .keys()
        .cloned()
        .collect()
}

#[test]
fn a_saved_branch_comes_back_byte_for_byte() {
    let dir = folder("roundtrip");
    let file = dir.join("job.eh3d");
    let who = Identity {
        name: Some("Board".into()),
        id: Some("board-1".into()),
    };
    eh3dfile::save(&file, &work(), &who, &cfg()).unwrap();

    let listing = eh3dfile::list(&file).unwrap();
    assert_eq!(
        (listing.name.as_str(), listing.id.as_str()),
        ("Board", "board-1")
    );
    assert_eq!(listing.rows.len(), 1);
    assert_eq!(
        (
            listing.rows[0].app.as_str(),
            listing.rows[0].format.as_str()
        ),
        ("pnp", "1.0")
    );
    assert!(listing.warnings.is_empty(), "{:?}", listing.warnings);

    let out = dir.join("out");
    let opened = eh3dfile::open(&file, Some(&out), &cfg()).unwrap();
    assert_eq!(opened.parts.as_deref(), Some("parts/board.pos"));
    assert!(opened.part_count.unwrap().0 > 0);
    assert!(opened.bom_count.unwrap().1 > 0);
    assert!(opened.feeder_count.unwrap() > 0);
    for (stored, source) in [
        ("parts/board.pos", "board.pos"),
        ("bom/bom.csv", "bom.csv"),
        ("feeders/feeders.csv", "feeders.csv"),
        ("settings/machine.cfg", "machine.cfg"),
    ] {
        assert_eq!(
            fs::read(out.join(stored)).unwrap(),
            fs::read(example(source)).unwrap(),
            "{stored}"
        );
    }

    // saving one table again replaces only that one
    let again = Inputs {
        parts: Some(example("centroid.csv")),
        ..Inputs::default()
    };
    eh3dfile::save(&file, &again, &Identity::default(), &cfg()).unwrap();
    let now = files(&file);
    assert!(now.contains(&"parts/centroid.csv".to_string()), "{now:?}");
    assert!(!now.contains(&"parts/board.pos".to_string()), "{now:?}");
    assert!(now.contains(&"bom/bom.csv".to_string()), "{now:?}");
}

#[test]
fn the_other_branches_keep_their_bytes_whatever_this_program_does() {
    let dir = folder("others");
    let file = dir.join("job.eh3d");
    synthetic(&file);
    let before = others(&file);
    assert!(before.contains_key("holography/data.bin"));

    eh3dfile::save(&file, &work(), &Identity::default(), &cfg()).unwrap();
    assert_eq!(others(&file), before);
    import(&file, &example("board.mnt"), Kind::Parts).unwrap();
    assert_eq!(others(&file), before);
    eh3dfile::take(&file, "pcb", "out/board.pos", None, &cfg()).unwrap();
    assert_eq!(others(&file), before);

    let listing = eh3dfile::list(&file).unwrap();
    let ids: Vec<&str> = listing.rows.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, ["holography", "pcb", "pnp", "x-vendor"]);
}

#[test]
fn an_import_of_each_kind_keeps_its_original_and_its_report() {
    let dir = folder("import");
    let file = dir.join("job.eh3d");
    let source = example("centroid.csv");
    let (report, _) = import(&file, &source, Kind::Parts).unwrap();
    let original = fs::read(&source).unwrap();
    assert_eq!(report.source.format, "csv");
    assert_eq!(report.source.sha256, sha256_hex(&original));
    assert_eq!(
        report.original.as_deref(),
        Some("origin/parts/centroid.csv")
    );
    assert!(!report.export_losses.is_empty());
    import(&file, &example("bom.csv"), Kind::Bom).unwrap();
    import(&file, &example("feeders.csv"), Kind::Feeders).unwrap();

    let out = dir.join("out");
    let opened = eh3dfile::open(&file, Some(&out), &cfg()).unwrap();
    assert!(opened.warnings.is_empty(), "{:?}", opened.warnings);
    assert_eq!(opened.parts.as_deref(), Some("parts/centroid.csv"));
    assert_eq!(opened.bom.as_deref(), Some("bom/bom.csv"));
    assert_eq!(opened.feeders.as_deref(), Some("feeders/feeders.csv"));
    let kinds: Vec<Kind> = opened.reports.iter().map(|(k, _)| *k).collect();
    assert_eq!(kinds, Kind::ALL);
    assert_eq!(opened.reports[0].1, report);
    for (stored, from) in [
        ("origin/parts/centroid.csv", "centroid.csv"),
        ("origin/bom/bom.csv", "bom.csv"),
        ("origin/feeders/feeders.csv", "feeders.csv"),
    ] {
        assert_eq!(
            fs::read(out.join(stored)).unwrap(),
            fs::read(example(from)).unwrap(),
            "{stored}"
        );
    }
    // the table of parts that was made is one the program reads, with the parts that the file leaves out
    let table = fs::read_to_string(out.join("parts/centroid.csv")).unwrap();
    assert!(
        table.starts_with("ref,val,package,x,y,rot,side,dnp\n"),
        "{table}"
    );
    let read = parse_all(&table, &cfg().profile).unwrap();
    assert_eq!(Some((read.parts.len(), read.dnp.len())), opened.part_count);
    assert!(!read.dnp.is_empty());

    // a second import of a kind replaces only that kind and says so
    let (_, notes) = import(&file, &example("board.mnt"), Kind::Parts).unwrap();
    assert!(notes.iter().any(|n| n.contains("replaced")), "{notes:?}");
    let now = files(&file);
    assert!(
        now.contains(&"origin/parts/board.mnt".to_string()),
        "{now:?}"
    );
    assert!(
        !now.contains(&"origin/parts/centroid.csv".to_string()),
        "{now:?}"
    );
    assert!(!now.contains(&"parts/centroid.csv".to_string()), "{now:?}");
    assert!(now.contains(&"origin/bom/bom.csv".to_string()), "{now:?}");
    assert!(eh3dfile::list(&file).unwrap().warnings.is_empty());
}

#[test]
fn a_copy_taken_from_pcb_is_told_as_out_of_date_and_taken_again() {
    let dir = folder("stale");
    let file = dir.join("job.eh3d");
    synthetic(&file);
    let (target, kind, _) = eh3dfile::take(&file, "pcb", "out/board.pos", None, &cfg()).unwrap();
    assert_eq!(target, "taken/pcb/out/board.pos");
    assert_eq!(kind, Some(Kind::Parts));
    let (_, kind, _) = eh3dfile::take(&file, "pcb", "out/bom.csv", None, &cfg()).unwrap();
    assert_eq!(kind, Some(Kind::Bom));
    let (_, kind, _) = eh3dfile::take(&file, "pcb", "board.bin", None, &cfg()).unwrap();
    assert_eq!(kind, None);

    let project = Project::open(&file, &OpenOptions::default()).unwrap();
    let record = project.branch("pnp").unwrap().record.clone().unwrap();
    assert_eq!(record.depends.len(), 3);
    for dep in &record.depends {
        assert_eq!(dep.branch, "pcb");
        assert_eq!(dep.hash, project.branch("pcb").unwrap().hash);
    }
    let mut artifacts: Vec<&str> = record.depends.iter().map(|d| d.artifact.as_str()).collect();
    artifacts.sort_unstable();
    assert_eq!(artifacts, ["bom", "file", "placements"]);
    drop(project);
    assert!(eh3dfile::list(&file).unwrap().warnings.is_empty());

    write_branch(&file, "pcb", &[("out/board.pos", b"# changed\n")]);
    let listing = eh3dfile::list(&file).unwrap();
    assert!(listing
        .warnings
        .iter()
        .any(|w| w.starts_with("OUT OF DATE")));
    let opened = eh3dfile::open(&file, None, &cfg()).unwrap();
    assert!(!opened.warnings.is_empty());
    assert_eq!(opened.parts.as_deref(), Some("taken/pcb/out/board.pos"));
}

#[test]
fn a_take_that_cannot_be_done_changes_nothing() {
    let dir = folder("refuse");
    let file = dir.join("job.eh3d");
    synthetic(&file);
    eh3dfile::save(&file, &work(), &Identity::default(), &cfg()).unwrap();
    let before = fs::read(&file).unwrap();
    for (branch, rel, kind) in [
        ("pnp", "parts/board.pos", None),
        ("pcb", "missing.pos", None),
        ("nowhere", "out/board.pos", None),
        ("pcb", "out/bom.csv", Some(Kind::Parts)),
        ("pcb", "board.bin", Some(Kind::Feeders)),
    ] {
        assert!(
            eh3dfile::take(&file, branch, rel, kind, &cfg()).is_err(),
            "{branch}/{rel}"
        );
        assert_eq!(fs::read(&file).unwrap(), before, "{branch}/{rel}");
    }
}

#[test]
fn a_file_that_the_program_cannot_read_creates_nothing() {
    let dir = folder("junk");
    let file = dir.join("job.eh3d");
    let junk = dir.join("junk.csv");
    fs::write(&junk, "nothing,of,use\n1,2,3\n").unwrap();
    for kind in Kind::ALL {
        assert!(
            import(&file, &text(&junk), kind).is_err(),
            "{}",
            kind.name()
        );
        assert!(!file.exists());
    }
    for inputs in [
        Inputs::default(),
        Inputs {
            feeders: Some(text(&junk)),
            ..Inputs::default()
        },
        Inputs {
            parts: Some(text(&dir.join("not_there.pos"))),
            ..Inputs::default()
        },
    ] {
        assert!(eh3dfile::save(&file, &inputs, &Identity::default(), &cfg()).is_err());
        assert!(!file.exists());
    }
    assert!(eh3dfile::open(&file, None, &cfg()).is_err());
}

#[test]
fn a_branch_of_a_newer_format_or_sealed_is_not_written() {
    let dir = folder("newer");
    let file = dir.join("job.eh3d");
    synthetic(&file);
    let project = Project::open(&file, &OpenOptions::default()).unwrap();
    let update = BranchUpdate::new("pnp", "2.0", "9.0.0").with_entry("pnp.json", b"{}");
    project
        .save_branch(&Writer::for_app("pnp"), update, &SaveOptions::default())
        .unwrap();
    let before = fs::read(&file).unwrap();
    let error = eh3dfile::save(&file, &work(), &Identity::default(), &cfg()).unwrap_err();
    assert!(error.contains("EH3D-PKG-024"), "{error}");
    assert_eq!(fs::read(&file).unwrap(), before);

    let sealed = dir.join("sealed.eh3d");
    synthetic(&sealed);
    let project = Project::open(&sealed, &OpenOptions::default()).unwrap();
    let mut update = BranchUpdate::new("pnp", "1.0", "0.1.0").with_entry("pnp.json", b"{}");
    update.seal = Some(Seal {
        by: "test".into(),
        at: "never".into(),
        reason: None,
    });
    project
        .save_branch(&Writer::for_app("pnp"), update, &SaveOptions::default())
        .unwrap();
    let before = fs::read(&sealed).unwrap();
    let error = eh3dfile::save(&sealed, &work(), &Identity::default(), &cfg()).unwrap_err();
    assert!(error.contains("EH3D-PKG-020"), "{error}");
    assert_eq!(fs::read(&sealed).unwrap(), before);
}

#[test]
fn damaged_projects_are_errors_and_never_a_crash() {
    let dir = folder("damaged");
    let file = dir.join("job.eh3d");
    synthetic(&file);
    eh3dfile::save(&file, &work(), &Identity::default(), &cfg()).unwrap();
    let good = fs::read(&file).unwrap();
    // a fixed seed: the same damage every run
    let mut seed: u64 = 0x5EED_0B0A;
    let mut next = move |limit: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % limit.max(1) as u64) as usize
    };
    let broken = dir.join("broken.eh3d");
    for round in 0..120 {
        let mut bytes = good.clone();
        if round % 2 == 0 {
            for _ in 0..=next(8) {
                let at = next(bytes.len());
                bytes[at] ^= 1 << next(8);
            }
        } else {
            bytes.truncate(next(bytes.len()));
        }
        fs::write(&broken, &bytes).unwrap();
        let _ = eh3dfile::list(&broken);
        let _ = eh3dfile::open(&broken, None, &cfg());
        let _ = eh3dfile::take(&broken, "pcb", "out/board.pos", None, &cfg());
        let _ = eh3dfile::save(&broken, &work(), &Identity::default(), &cfg());
    }
    fs::write(&broken, b"this is not a project").unwrap();
    assert!(eh3dfile::list(&broken).is_err());
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .arg("project")
        .args(args)
        .output()
        .unwrap()
}

fn said(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn the_command_saves_imports_takes_lists_opens_and_extracts() {
    let dir = folder("cli");
    let path = dir.join("job.eh3d");
    synthetic(&path);
    let file = text(&path);
    let (pos, bom, feeders) = (
        example("board.pos"),
        example("bom.csv"),
        example("feeders.csv"),
    );

    let o = run(&["save", &file, "--parts", &pos, "--feeders", &feeders]);
    assert_eq!(o.status.code(), Some(0), "{}", said(&o.stderr));
    let o = run(&["import", &file, &bom, "--kind", "bom"]);
    assert_eq!(o.status.code(), Some(0), "{}", said(&o.stderr));
    assert!(
        said(&o.stdout).contains("imported bom.csv as bom"),
        "{}",
        said(&o.stdout)
    );
    let o = run(&["take", &file, "pcb", "out/board.pos"]);
    assert_eq!(o.status.code(), Some(0), "{}", said(&o.stderr));
    assert!(
        said(&o.stdout).contains("now the parts"),
        "{}",
        said(&o.stdout)
    );

    let o = run(&["list", &file, "--json"]);
    assert_eq!(o.status.code(), Some(0), "{}", said(&o.stderr));
    let document: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(document["branches"]
        .as_array()
        .unwrap()
        .iter()
        .any(|b| b["id"] == "pnp"));

    let into = text(&dir.join("into"));
    let o = run(&["open", &file, "--into", &into]);
    assert_eq!(o.status.code(), Some(0), "{}", said(&o.stderr));
    assert!(
        said(&o.stdout).contains("parts: taken/pcb/out/board.pos"),
        "{}",
        said(&o.stdout)
    );
    // what is there is never written over
    assert_eq!(
        run(&["extract", &file, "pnp", &into]).status.code(),
        Some(1)
    );
    let other = text(&dir.join("other"));
    assert_eq!(
        run(&["extract", &file, "pnp", &other]).status.code(),
        Some(0)
    );

    // a source that changed is a check that is not passed
    write_branch(&path, "pcb", &[("out/board.pos", b"# changed\n")]);
    let o = run(&["list", &file]);
    assert_eq!(o.status.code(), Some(2), "{}", said(&o.stderr));
    assert!(
        said(&o.stderr).contains("OUT OF DATE"),
        "{}",
        said(&o.stderr)
    );
}

#[test]
fn a_wrong_request_of_the_command_creates_nothing() {
    let dir = folder("wrong");
    let file = text(&dir.join("a.eh3d"));
    let other = text(&dir.join("a.csv"));
    let pos = example("board.pos");
    for args in [
        vec![],
        vec!["melt", &file],
        vec!["save", &file],
        vec!["save", &other, "--parts", &pos],
        vec!["save", &file, "--parts", &pos, "--oops"],
        vec!["import", &file, &pos],
        vec!["import", &file, &pos, "--kind", "nozzles"],
        vec!["import", &file, &pos, "--kind", "bom", "--format", "csv"],
        vec!["open", &file],
        vec!["take", &file, "pcb", "out/board.pos"],
        vec!["list"],
    ] {
        let o = run(&args);
        assert_eq!(o.status.code(), Some(1), "{args:?}: {}", said(&o.stderr));
        assert!(said(&o.stderr).starts_with("error:"), "{args:?}");
        assert!(!Path::new(&file).exists(), "{args:?}");
        assert!(!Path::new(&other).exists(), "{args:?}");
    }
}
