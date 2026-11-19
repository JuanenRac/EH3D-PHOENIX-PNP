//! The report of a job as a file to keep: what each feeder gave and what became of its parts, the life of the nozzle tips with what the job added to it, the warnings, and the same in the
//! text, the page of HTML, the CSV and the JSON. The command writes them with `simulate --report-html`, `--report-text`, `--report` and `--report-csv` beside `--nozzle-life`.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::job::Job;
use eh3d_phoenix_pnp::nozzlelife::NozzleLife;
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use eh3d_phoenix_pnp::report::{self, build, ReportInput};
use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Output};

const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nR3,10k,R,30,10,180,top\nC1,100n,C,10,20,0,top\nC2,100n,C,20,20,0,top\nU1,MCU,Q,40,15,0,top\n";
const FEEDERS: &str =
    "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\n";
const PACKAGES: &str = "package,nozzle_tip\nR,N45\nC,N45\nQ,N75\n";

fn job(extra: &str) -> Job {
    let cfg = MachineConfig::parse(&format!(
        "tip_changer_x_mm = -50\ntip_changer_y_mm = 0\n{extra}"
    ))
    .unwrap();
    let parts = from_csv(PARTS).unwrap();
    let feeders = feeding::from_csv(FEEDERS).unwrap();
    let picks = assign(&parts, &feeders, Some(Side::Top));
    let mut catalog = Catalog::new();
    catalog.add_packages(PACKAGES).unwrap();
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

fn life(text: &str) -> NozzleLife {
    NozzleLife::parse(text).unwrap()
}

fn input(record: &str) -> ReportInput {
    ReportInput {
        life: Some(life(record)),
        ..ReportInput::default()
    }
}

// ---- the parts that did not get placed, feeder by feeder

#[test]
fn the_parts_put_aside_and_left_out_are_counted_for_the_feeder_they_were_to_come_from() {
    // every pick fails, the person's policy puts the parts aside
    let mut j =
        job("error_policy = defer\nsim_pick_fail_rate = 1\nfault_limit = 0\npick_retries = 0\n");
    j.run(None).unwrap();
    let r = build(&j, &ReportInput::default());
    let mut counts: Vec<(String, usize, usize)> = r.feeder_rejects.clone();
    counts.sort();
    assert_eq!(
        counts,
        [
            ("F1".to_string(), 3, 0),
            ("F2".to_string(), 2, 0),
            ("F3".to_string(), 1, 0)
        ]
    );
    assert_eq!(r.rejects_of("F1"), (3, 0));
    assert_eq!(r.rejects_of("nothing"), (0, 0));
    // the table of the feeders names them
    let text = r.text(Lang::En);
    let section: Vec<&str> = text
        .lines()
        .skip_while(|l| *l != "By feeder")
        .take(8)
        .collect();
    assert!(section.join("\n").contains("Put aside"), "{text}");
    assert!(
        section
            .iter()
            .any(|l| l.trim_start().starts_with("F1") && l.contains('3')),
        "{section:?}"
    );
    // a job that went well has none
    let mut ok = job("");
    ok.run(None).unwrap();
    let r = build(&ok, &ReportInput::default());
    assert!(r.feeder_rejects.iter().all(|f| f.1 == 0 && f.2 == 0));
    let json: Value = serde_json::from_str(&report::json(&ok, &ReportInput::default())).unwrap();
    let feeders = json["work"]["feeders"].as_array().unwrap();
    assert_eq!(feeders.len(), 3);
    let f1 = feeders.iter().find(|f| f["id"] == "F1").unwrap();
    assert_eq!(
        (f1["picked"].as_u64(), f1["placed"].as_u64()),
        (Some(3), Some(3))
    );
    assert_eq!(
        (f1["put_aside"].as_u64(), f1["left_out"].as_u64()),
        (Some(0), Some(0))
    );
    assert_eq!(f1["switched_off"], false);
}

// ---- the life of the tips

#[test]
fn the_tips_show_what_the_job_did_and_what_the_record_said_before() {
    let mut j = job("nozzle_life_warn = 100");
    j.run(None).unwrap();
    // without a record only this job is known
    let r = build(&j, &ReportInput::default());
    let tips: Vec<(&str, u64, Option<u64>, bool)> = r
        .tip_life
        .iter()
        .map(|t| (t.tip.as_str(), t.parts, t.after, t.worn))
        .collect();
    assert_eq!(tips, [("N45", 5, None, false), ("N75", 1, None, false)]);
    // with a record the uses are added, and a tip that gets to the limit is worn
    let r = build(
        &j,
        &input("tip,uses,mounts,replaced\nN45,96,10,0\nN75,10,2,0\n"),
    );
    let by = |name: &str| r.tip_life.iter().find(|t| t.tip == name).unwrap().clone();
    assert_eq!(
        (by("N45").before, by("N45").after, by("N45").worn),
        (Some(96), Some(101), true)
    );
    assert_eq!(
        (by("N75").before, by("N75").after, by("N75").worn),
        (Some(10), Some(11), false)
    );
    // a tip the record does not know starts from nothing
    let r = build(&j, &input("tip,uses,mounts,replaced\nN99,5,1,0\n"));
    assert_eq!(
        r.tip_life.iter().find(|t| t.tip == "N45").unwrap().before,
        Some(0)
    );
    // the limit 0 never warns
    let mut quiet = job("nozzle_life_warn = 0");
    quiet.run(None).unwrap();
    let r = build(
        &quiet,
        &input("tip,uses,mounts,replaced\nN45,999999,10,0\n"),
    );
    assert!(r.tip_life.iter().all(|t| !t.worn));
}

#[test]
fn a_tip_that_is_worn_is_a_finding_and_a_row_in_every_form_of_the_report() {
    let mut j = job("nozzle_life_warn = 100");
    j.run(None).unwrap();
    let r = build(
        &j,
        &input("tip,uses,mounts,replaced\nN45,96,10,0\nN75,10,2,0\n"),
    );
    for lang in Lang::ALL {
        let text = r.text(lang);
        assert!(
            text.contains("N45") && text.contains("101"),
            "{lang:?}\n{text}"
        );
        let page = r.html(lang);
        assert!(page.contains("N45") && !page.to_lowercase().contains("<script"));
    }
    let text = r.text(Lang::En);
    assert!(text.contains("Life of the nozzle tips"), "{text}");
    assert!(
        text.contains("WARNING tip N45: 101 parts placed with this job, at or over the 100 that nozzle_life_warn allows"),
        "{text}"
    );
    assert!(!text.contains("WARNING tip N75"), "{text}");
    // the other languages have their own words
    let words: Vec<String> = Lang::ALL
        .iter()
        .map(|l| {
            r.text(*l)
                .lines()
                .find(|x| x.contains("N45") && x.contains("101"))
                .map(str::to_string)
                .unwrap_or_default()
        })
        .collect();
    assert!(words.iter().all(|w| !w.is_empty()));
    // the JSON
    let json: Value = serde_json::from_str(&report::json(
        &j,
        &input("tip,uses,mounts,replaced\nN45,96,10,0\n"),
    ))
    .unwrap();
    assert_eq!(json["work"]["nozzle_life"]["warn_at"], 100);
    let tips = json["work"]["nozzle_life"]["tips"].as_array().unwrap();
    let n45 = tips.iter().find(|t| t["tip"] == "N45").unwrap();
    assert_eq!(
        (
            n45["parts"].as_u64(),
            n45["uses_before"].as_u64(),
            n45["uses_after"].as_u64(),
            n45["worn"].as_bool()
        ),
        (Some(5), Some(96), Some(101), Some(true))
    );
    // with no record the uses before are null
    let json: Value = serde_json::from_str(&report::json(&j, &ReportInput::default())).unwrap();
    let n45 = json["work"]["nozzle_life"]["tips"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["tip"] == "N45")
        .unwrap()
        .clone();
    assert!(n45["uses_before"].is_null() && n45["uses_after"].is_null());
    // the CSV: the old rows and sections stay, the new ones follow
    let csv = report::csv(&j, &input("tip,uses,mounts,replaced\nN45,96,10,0\n"));
    assert!(csv.contains("\nlife,N45,101,,,5,,,1\n"), "{csv}");
    assert!(csv.contains("\nlife,N75,1,,,1,,,0\n"), "{csv}");
    assert!(csv.contains("\nrejected,F1,0,,,,,0,\n"), "{csv}");
    let header_columns = csv.lines().next().unwrap().split(',').count();
    for line in csv
        .lines()
        .filter(|l| l.starts_with("life,") || l.starts_with("rejected,"))
    {
        assert_eq!(line.split(',').count(), header_columns, "{line}");
    }
}

#[test]
fn every_word_of_the_new_sections_is_in_the_eight_languages() {
    assert!(report::words_complete());
    let mut j = job("");
    j.run(None).unwrap();
    let r = build(&j, &input("tip,uses,mounts,replaced\nN45,1,1,0\n"));
    for lang in Lang::ALL {
        let text = r.text(lang);
        assert!(text.lines().any(|l| l.contains("N45")), "{lang:?}");
        // the title of the section is not the English one in the languages that are not English or Portuguese
        if lang != Lang::En && lang != Lang::Pt {
            assert!(!text.contains("Life of the nozzle tips"), "{lang:?}");
        }
    }
}

// ---- the command

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .current_dir(root())
        .output()
        .unwrap()
}

fn text(o: &[u8]) -> String {
    String::from_utf8_lossy(o).into_owned()
}

#[test]
fn simulate_writes_the_report_with_the_tips_and_the_record_before_the_job() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_jobreport_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let p = |name: &str| dir.join(name).to_string_lossy().into_owned();
    std::fs::write(dir.join("packages.csv"), PACKAGES).unwrap();
    std::fs::write(dir.join("parts.csv"), PARTS).unwrap();
    std::fs::write(dir.join("feeders.csv"), FEEDERS).unwrap();
    std::fs::write(
        dir.join("life.csv"),
        "tip,uses,mounts,replaced\nN45,499996,10,0\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("m.cfg"),
        "tip_changer_x_mm = -50\ntip_changer_y_mm = 0\nnozzle_life_warn = 500000\n",
    )
    .unwrap();
    let args = |extra: &[&str]| -> Vec<String> {
        let mut a: Vec<String> = [
            "simulate",
            &p("parts.csv"),
            &p("feeders.csv"),
            "--config",
            &p("m.cfg"),
            "--packages",
            &p("packages.csv"),
            "--nozzle-life",
            &p("life.csv"),
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        a.extend(extra.iter().map(|s| s.to_string()));
        a
    };
    let go = |extra: &[&str]| -> Output {
        let a = args(extra);
        let refs: Vec<&str> = a.iter().map(String::as_str).collect();
        run(&refs)
    };
    let o = go(&[
        "--report-html",
        &p("report.html"),
        "--report",
        &p("report.json"),
        "--report-text",
        &p("report.txt"),
        "--report-csv",
        &p("report.csv"),
    ]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    // the page holds the tips and the warning, in one file with nothing from outside
    let page = std::fs::read_to_string(dir.join("report.html")).unwrap();
    assert!(
        page.contains("Life of the nozzle tips")
            && page.contains("499996")
            && page.contains("500001"),
        "{page}"
    );
    assert!(page.contains("WARNING tip N45"), "{page}");
    assert!(!page.to_lowercase().contains("<script") && !page.contains("http"));
    let json: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("report.json")).unwrap()).unwrap();
    let n45 = json["work"]["nozzle_life"]["tips"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["tip"] == "N45")
        .unwrap()
        .clone();
    assert_eq!(
        (
            n45["uses_before"].as_u64(),
            n45["uses_after"].as_u64(),
            n45["worn"].as_bool()
        ),
        (Some(499_996), Some(500_001), Some(true))
    );
    let txt = std::fs::read_to_string(dir.join("report.txt")).unwrap();
    assert!(txt.contains("WARNING tip N45"), "{txt}");
    let csv = std::fs::read_to_string(dir.join("report.csv")).unwrap();
    assert!(csv.contains("life,N45,500001,"), "{csv}");
    // the record of the tips was written after the job, but the report says what it was before
    let after = std::fs::read_to_string(dir.join("life.csv")).unwrap();
    assert!(after.contains("N45,500001,"), "{after}");
    // the page is never written over a file
    let o = go(&["--report-html", &p("report.html")]);
    assert!(!o.status.success());
    // without a record the report still names the tips of the job
    let o = run(&[
        "simulate",
        &p("parts.csv"),
        &p("feeders.csv"),
        "--config",
        &p("m.cfg"),
        "--packages",
        &p("packages.csv"),
        "--report-text",
        &p("plain.txt"),
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let plain = std::fs::read_to_string(dir.join("plain.txt")).unwrap();
    assert!(
        plain.contains("Life of the nozzle tips") && plain.contains("N45"),
        "{plain}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
