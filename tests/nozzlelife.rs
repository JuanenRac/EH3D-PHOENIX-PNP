//! The use of the nozzle tips: the file that keeps it, what a job adds to it, the warning level, and the commands `nozzle-life` and `simulate --nozzle-life`.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::job::Job;
use eh3d_phoenix_pnp::nozzlelife::{
    read_file, tip_name_ok, update_file, uses_of_job, write_file, NozzleLife, MAX_TIPS,
};
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use std::path::PathBuf;
use std::process::{Command, Output};

const EXAMPLE: &str = include_str!("../examples/nozzle_life.csv");
const MACHINE: &str = include_str!("../examples/machine.cfg");
const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nR3,10k,R,30,10,180,top\nC1,100n,C,10,20,0,top\nC2,100n,C,20,20,0,top\nU1,MCU,Q,40,15,0,top\n";
const FEEDERS: &str =
    "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\n";
const PACKAGES: &str = "package,nozzle_tip\nR,N45\nC,N45\nQ,N75\n";

fn job_with(packages: &str) -> Job {
    let cfg = MachineConfig::parse("tip_changer_x_mm = -50\ntip_changer_y_mm = 0").unwrap();
    let parts = from_csv(PARTS).unwrap();
    let feeders = feeding::from_csv(FEEDERS).unwrap();
    let picks = assign(&parts, &feeders, Some(Side::Top));
    let mut catalog = Catalog::new();
    if !packages.is_empty() {
        catalog.add_packages(packages).unwrap();
    }
    let plan = plan_advanced(
        &parts,
        &feeders,
        &picks,
        &catalog,
        &cfg,
        &Options::default(),
    )
    .unwrap();
    Job::new(plan, feeders, cfg).unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_life_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

fn text(o: &[u8]) -> String {
    String::from_utf8_lossy(o).into_owned()
}

#[test]
fn the_example_is_read_and_written_back_as_it_is() {
    let life = NozzleLife::parse(EXAMPLE).unwrap();
    let rows: Vec<(&str, u64, u64, u32)> = life
        .tips()
        .map(|t| (t.tip.as_str(), t.uses, t.mounts, t.replaced))
        .collect();
    assert_eq!(rows, [("N45", 120_000, 40, 1), ("N75", 510_000, 12, 0)]);
    assert_eq!(life.to_csv(), EXAMPLE.replace("\r\n", "\n"));
    assert_eq!(NozzleLife::parse(&life.to_csv()).unwrap(), life);
    assert_eq!(life.get(" N45 ").unwrap().uses, 120_000);
    assert!(life.get("N99").is_none() && !life.is_empty());
}

#[test]
fn notes_a_byte_order_mark_and_another_delimiter_are_read() {
    let with_notes =
        "\u{feff}# the tips of the machine\ntip;uses;mounts;replaced\n# a note\nN45;7;1;0\n";
    let life = NozzleLife::parse(with_notes).unwrap();
    assert_eq!(life.get("N45").unwrap().uses, 7);
    // the identity of the last job is a note that is kept
    let tagged = "# last_job = 00ff00ff00ff00ff\ntip,uses,mounts,replaced\nN45,7,1,0\n";
    let life = NozzleLife::parse(tagged).unwrap();
    assert_eq!(life.last_job(), "00ff00ff00ff00ff");
    assert_eq!(life.to_csv(), tagged);
    // an identity that is not letters and digits is ignored
    let odd = NozzleLife::parse("# last_job = ../etc\ntip,uses,mounts,replaced\n").unwrap();
    assert_eq!(odd.last_job(), "");
}

#[test]
fn a_file_that_is_not_a_record_of_the_tips_is_refused_with_the_row() {
    for (input, what) in [
        ("", "empty"),
        ("# only a note\n", "only notes"),
        ("tip,uses\nN45,1\n", "a header with two columns"),
        (
            "tip,uses,mounts,replaced\nN45,1,1\n",
            "a row with three fields",
        ),
        (
            "tip,uses,mounts,replaced\nN45,x,1,0\n",
            "uses is not a number",
        ),
        ("tip,uses,mounts,replaced\nN45,-1,1,0\n", "uses is negative"),
        (
            "tip,uses,mounts,replaced\nN45,1,1,4294967296\n",
            "replaced is too large",
        ),
        (
            "tip,uses,mounts,replaced\nN45,1000000000001,1,0\n",
            "uses is too large",
        ),
        (
            "tip,uses,mounts,replaced\nN45,1,1,0\nN45,2,1,0\n",
            "a tip is listed twice",
        ),
        ("tip,uses,mounts,replaced\n,1,1,0\n", "a tip without a name"),
        (
            "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\n",
            "a table of parts",
        ),
    ] {
        let e = NozzleLife::parse(input).unwrap_err();
        assert_eq!(e.code, Code::NozzleLifeBad, "{what}");
    }
    let e = NozzleLife::parse("tip,uses,mounts,replaced\nN45,1,1,0\nN75,y,1,0\n").unwrap_err();
    assert!(e.detail.contains("row 3"), "{}", e.detail);
    let many = format!(
        "tip,uses,mounts,replaced\n{}",
        (0..=MAX_TIPS)
            .map(|n| format!("T{n},1,1,0\n"))
            .collect::<String>()
    );
    assert_eq!(
        NozzleLife::parse(&many).unwrap_err().code,
        Code::NozzleLifeBad
    );
}

#[test]
fn the_name_of_a_tip_is_a_short_text_without_control_characters_commas_or_quotes() {
    assert!(tip_name_ok("N45") && tip_name_ok(" Ø 0.4 mm "));
    assert!(!tip_name_ok("") && !tip_name_ok("   "));
    assert!(!tip_name_ok("a,b") && !tip_name_ok("a\"b") && !tip_name_ok("a\tb"));
    assert!(tip_name_ok(&"x".repeat(64)) && !tip_name_ok(&"x".repeat(65)));
    let mut life = NozzleLife::default();
    assert!(!life.add("a,b", 1, 1) && life.is_empty());
}

#[test]
fn uses_are_added_a_tip_is_reset_and_the_warning_level_is_told() {
    let mut life = NozzleLife::parse(EXAMPLE).unwrap();
    assert!(life.add("N45", 10, 1) && life.add("N30", 5, 1));
    assert_eq!(life.get("N45").unwrap().uses, 120_010);
    assert_eq!(life.get("N30").unwrap().mounts, 1);
    // the level: 0 warns of nothing, a tip at the level is told, one below it is not
    assert!(life.over(0).is_empty());
    let over: Vec<&str> = life.over(510_000).iter().map(|t| t.tip.as_str()).collect();
    assert_eq!(over, ["N75"]);
    assert!(life.over(510_001).is_empty());
    let lines = life.warning_lines(500_000);
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].starts_with("warning: nozzle tip N75 has placed 510000 parts"),
        "{lines:?}"
    );
    assert!(life.text(500_000).contains(
        "tip N75: 510000 parts placed, mounted 12 times, replaced 0 times <- look at this tip"
    ));
    assert!(!life.text(0).contains("look at this tip"));
    // a reset is the only way a count goes down
    assert!(life.reset(" N75 ") && !life.reset("N99"));
    let n75 = life.get("N75").unwrap();
    assert_eq!((n75.uses, n75.mounts, n75.replaced), (0, 0, 1));
    assert!(life.over(500_000).is_empty());
    assert_eq!(NozzleLife::default().text(1), "no tip has been used yet\n");
    // a count stops at the largest
    life.add("N30", u64::MAX, u64::MAX);
    assert_eq!(life.get("N30").unwrap().uses, 1_000_000_000_000);
}

#[test]
fn a_job_adds_the_parts_each_tip_placed_and_only_the_ones_that_were_placed() {
    let mut j = job_with(PACKAGES);
    j.run(None).unwrap();
    let uses = uses_of_job(&j);
    let by: Vec<(&str, u64)> = uses.iter().map(|(t, u, _)| (t.as_str(), *u)).collect();
    assert_eq!(by, [("N45", 5), ("N75", 1)]);
    assert!(uses.iter().all(|(_, _, m)| *m >= 1));
    // a job that stopped early counts what it placed
    let mut part = job_with(PACKAGES);
    part.run(Some(2)).unwrap();
    let placed: u64 = uses_of_job(&part).iter().map(|(_, u, _)| *u).sum();
    assert_eq!(placed as usize, part.stats().placed);
    // without a tip in the plan nothing is counted
    let mut plain = job_with("");
    plain.run(None).unwrap();
    assert!(uses_of_job(&plain).is_empty());
}

#[test]
fn the_file_is_written_whole_the_one_it_replaces_is_kept_and_a_job_is_counted_once() {
    let dir = scratch("file");
    let path = dir.join("machine.life.csv").to_string_lossy().into_owned();
    // a file that is not there is an empty record
    assert!(read_file(&path, 1_000_000).unwrap().is_empty());
    let uses = vec![("N45".to_string(), 5, 1), ("N75".to_string(), 1, 1)];
    let first = update_file(&path, "00000000000000aa", &uses, 1_000_000)
        .unwrap()
        .unwrap();
    assert_eq!(first.get("N45").unwrap().uses, 5);
    assert!(!PathBuf::from(format!("{path}.prev")).exists());
    assert!(!PathBuf::from(format!("{path}.tmp")).exists());
    // the same job again is not counted again
    assert_eq!(
        update_file(&path, "00000000000000aa", &uses, 1_000_000).unwrap(),
        None
    );
    assert_eq!(read_file(&path, 1_000_000).unwrap(), first);
    // another job adds to it, and the file it replaced is kept
    let second = update_file(&path, "00000000000000bb", &uses, 1_000_000)
        .unwrap()
        .unwrap();
    assert_eq!(second.get("N45").unwrap().uses, 10);
    assert_eq!(second.last_job(), "00000000000000bb");
    let prev = std::fs::read_to_string(format!("{path}.prev")).unwrap();
    assert_eq!(NozzleLife::parse(&prev).unwrap(), first);
    // a tip that cannot be named is refused and the file is left as it was
    let before = std::fs::read_to_string(&path).unwrap();
    let e = update_file(
        &path,
        "00000000000000cc",
        &[("a,b".to_string(), 1, 1)],
        1_000_000,
    )
    .unwrap_err();
    assert_eq!(e.code, Code::NozzleLifeBad);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_that_is_not_a_record_is_never_written_over_and_a_big_one_is_refused() {
    let dir = scratch("foreign");
    let path = dir.join("parts.csv").to_string_lossy().into_owned();
    std::fs::write(&path, PARTS).unwrap();
    let e = update_file(
        &path,
        "00000000000000aa",
        &[("N45".to_string(), 1, 1)],
        1_000_000,
    )
    .unwrap_err();
    assert_eq!(e.code, Code::NozzleLifeBad);
    assert!(e.detail.contains("parts.csv"), "{}", e.detail);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), PARTS);
    assert_eq!(read_file(&path, 10).unwrap_err().code, Code::FileTooBig);
    // a folder that is not there cannot be written to, and says so
    let missing = dir
        .join("no")
        .join("such.csv")
        .to_string_lossy()
        .into_owned();
    let e = write_file(&missing, &NozzleLife::default()).unwrap_err();
    assert_eq!(e.code, Code::NozzleLifeBad);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_command_lists_the_tips_and_fails_while_one_is_at_the_level() {
    let o = run(&["nozzle-life", "examples/nozzle_life.csv"]);
    assert_eq!(o.status.code(), Some(2));
    let out = text(&o.stdout);
    assert!(
        out.contains("tip N45: 120000 parts placed, mounted 40 times, replaced 1 times\n"),
        "{out}"
    );
    assert!(
        out.contains("tip N75: 510000 parts placed") && out.contains("<- look at this tip"),
        "{out}"
    );
    assert!(
        text(&o.stderr).contains("1 tips have placed 500000 parts or more"),
        "{}",
        text(&o.stderr)
    );
    // a level above the use of every tip, or no level at all
    for level in ["600000", "0"] {
        let ok = run(&["nozzle-life", "examples/nozzle_life.csv", "--warn", level]);
        assert!(ok.status.success(), "{level}: {}", text(&ok.stderr));
    }
    let none = run(&["nozzle-life", "examples/no_such_file.csv"]);
    assert_eq!(none.status.code(), Some(1));
    let junk = run(&["nozzle-life", "examples/board.pos"]);
    assert_eq!(junk.status.code(), Some(1));
    assert!(
        text(&junk.stderr).contains("PHOENIX-PNP-059"),
        "{}",
        text(&junk.stderr)
    );
    let es = run(&["nozzle-life", "examples/board.pos", "--lang", "es"]);
    assert!(
        text(&es.stderr).contains("El archivo del uso de las puntas de boquilla no es válido"),
        "{}",
        text(&es.stderr)
    );
}

#[test]
fn a_tip_that_was_replaced_is_reset_on_purpose() {
    let dir = scratch("reset");
    let path = dir.join("life.csv").to_string_lossy().into_owned();
    std::fs::write(&path, EXAMPLE).unwrap();
    let o = run(&["nozzle-life", &path, "--reset", "N75"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        text(&o.stdout).contains("ok: the tip N75 was replaced"),
        "{}",
        text(&o.stdout)
    );
    let life = read_file(&path, 1_000_000).unwrap();
    let n75 = life.get("N75").unwrap();
    assert_eq!((n75.uses, n75.replaced), (0, 1));
    assert_eq!(life.get("N45").unwrap().uses, 120_000);
    // the file that was replaced is kept, and a tip that is not in the record is refused
    let prev = std::fs::read_to_string(format!("{path}.prev")).unwrap();
    assert_eq!(prev, EXAMPLE);
    let o = run(&["nozzle-life", &path, "--reset", "N99"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(text(&o.stderr).contains("N99"), "{}", text(&o.stderr));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn simulate_adds_a_finished_job_to_the_file_once_and_leaves_an_unfinished_one_alone() {
    let dir = scratch("simulate");
    let path = dir.join("machine.life.csv").to_string_lossy().into_owned();
    let base = [
        "simulate",
        "examples/board.pos",
        "examples/feeders.csv",
        "--config",
        "examples/machine.cfg",
        "--packages",
        "examples/packages.csv",
        "--nozzle-life",
        path.as_str(),
    ];
    // a job stopped after a few steps does not touch the file
    let mut early: Vec<&str> = base.to_vec();
    early.extend(["--steps", "3"]);
    let o = run(&early);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        text(&o.stdout).contains("did not run to its end"),
        "{}",
        text(&o.stdout)
    );
    assert!(!PathBuf::from(&path).exists());
    // a finished one adds its parts
    let o = run(&base);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    assert!(out.contains("nozzle life: tip N45: 5 more parts"), "{out}");
    assert!(out.contains("nozzle life: tip N75: 1 more parts"), "{out}");
    let life = read_file(&path, 1_000_000).unwrap();
    assert_eq!(
        (life.get("N45").unwrap().uses, life.get("N75").unwrap().uses),
        (5, 1)
    );
    // the same job is not counted a second time
    let again = run(&base);
    assert!(
        text(&again.stdout).contains("is counted in"),
        "{}",
        text(&again.stdout)
    );
    assert_eq!(read_file(&path, 1_000_000).unwrap(), life);
    // a file of something else is refused and left as it was
    let foreign = dir.join("not_a_record.csv").to_string_lossy().into_owned();
    std::fs::write(&foreign, "a,b\n1,2\n").unwrap();
    let mut other: Vec<&str> = base.to_vec();
    other.pop();
    other.push(foreign.as_str());
    let o = run(&other);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        text(&o.stderr).contains("PHOENIX-PNP-059"),
        "{}",
        text(&o.stderr)
    );
    assert_eq!(std::fs::read_to_string(&foreign).unwrap(), "a,b\n1,2\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_warning_level_is_a_setting_with_its_range_and_its_example() {
    assert!(KEYS.contains(&"nozzle_life_warn"));
    assert!(MACHINE.contains("\nnozzle_life_warn = "));
    assert_eq!(
        MachineConfig::parse(MACHINE).unwrap().nozzle_life_warn,
        500_000
    );
    assert_eq!(MachineConfig::default().nozzle_life_warn, 500_000);
    for ok in ["0", "1", "1000000000"] {
        assert!(
            MachineConfig::parse(&format!("nozzle_life_warn = {ok}")).is_ok(),
            "{ok}"
        );
    }
    for bad in ["-1", "1000000001"] {
        assert_eq!(
            MachineConfig::parse(&format!("nozzle_life_warn = {bad}"))
                .unwrap_err()
                .code,
            Code::ConfigOutOfRange,
            "{bad}"
        );
    }
    assert_eq!(
        MachineConfig::parse("nozzle_life_warn = many")
            .unwrap_err()
            .code,
        Code::ConfigBadValue
    );
    assert_eq!(Code::NozzleLifeBad.number(), 59);
    assert_eq!(Code::NozzleLifeBad.label(), "PHOENIX-PNP-059");
}
