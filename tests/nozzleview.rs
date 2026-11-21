//! What each nozzle is doing, in the places that read the journal and the job: the column of the state in a journal that is read back, the light of the signal tower with the nozzle that made
//! it, and the Job tab with the state of every nozzle. A fixed seed makes the simulated journal the same every time.

use eh3d_phoenix_pnp::busy::read_journal;
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::gui::app::AppState;
use eh3d_phoenix_pnp::gui::jobtab::{job_rows, nozzle_line, nozzles_now, JobRow};
use eh3d_phoenix_pnp::gui::samples;
use eh3d_phoenix_pnp::gui::settings::UiSettings;
use eh3d_phoenix_pnp::gui::texts::{tr, Text};
use eh3d_phoenix_pnp::journal::NozzleState;
use eh3d_phoenix_pnp::parts::Side;
use eh3d_phoenix_pnp::tower::{self, Light};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const HEADER: &str = "step,at_s,took_s,action,reference,feeder,nozzle,result,status,nozzle_state";

fn journal(rows: &[&str]) -> String {
    format!("{HEADER}\n{}\n", rows.join("\n"))
}

// ---- the journal read back

#[test]
fn the_state_of_the_nozzle_is_read_from_its_column_and_an_unknown_word_is_refused() {
    let steps = read_journal(&journal(&[
        "1,0,1,pick,R1,F1,1,ok,pending,carrying",
        "2,1,1,place,R1,F1,1,ok,placed,idle",
    ]))
    .unwrap();
    assert_eq!(steps[0].nozzle_state, "carrying");
    assert_eq!(steps[1].nozzle_state, "idle");
    // an older journal has no such column
    let old = read_journal(
        "step,at_s,took_s,action,reference,feeder,nozzle,result,status\n1,0,1,pick,R1,F1,1,ok,pending\n",
    )
    .unwrap();
    assert_eq!(old[0].nozzle_state, "");
    let e = read_journal(&journal(&["1,0,1,pick,R1,F1,1,ok,pending,asleep"])).unwrap_err();
    assert!(e.to_string().contains("not a state of a nozzle"), "{e}");
    assert!(e.to_string().contains("line 2"), "{e}");
    // every state the program writes is one it reads
    for s in NozzleState::ALL {
        let row = format!("1,0,1,pick,R1,F1,1,ok,pending,{}", s.name());
        assert_eq!(
            read_journal(&journal(&[&row])).unwrap()[0].nozzle_state,
            s.name()
        );
    }
}

// ---- the signal tower

const FAILING: [&str; 4] = [
    "1,0,1,pick,R1,F1,1,ok,pending,carrying",
    "2,1,1,place,R1,F1,1,ok,placed,idle",
    "3,2,1,pick,R2,F1,2,PHOENIX-PNP-023,pending,idle",
    "4,3,1,place,R3,F1,1,PHOENIX-PNP-024,pending,carrying",
];

#[test]
fn a_light_that_is_not_green_names_the_nozzle_and_what_it_was_doing() {
    let steps = read_journal(&journal(&FAILING)).unwrap();
    let t = tower::timeline(&steps, 2.0, 3.0);
    let red: Vec<_> = t.iter().filter(|i| i.light == Light::Red).collect();
    // the two failures are two intervals: they are not the same nozzle in the same state
    assert_eq!(red.len(), 2, "{t:?}");
    assert_eq!((red[0].nozzle, red[0].nozzle_state.as_str()), (2, "idle"));
    assert_eq!(
        (red[1].nozzle, red[1].nozzle_state.as_str()),
        (1, "carrying")
    );
    let green: Vec<_> = t.iter().filter(|i| i.light == Light::Green).collect();
    assert!(green
        .iter()
        .all(|i| i.nozzle == 0 && i.nozzle_state.is_empty()));
    let text = tower::text(&t);
    assert!(
        text.contains("2.000 s to 3.000 s: red (failed) - nozzle 2: idle"),
        "{text}"
    );
    assert!(
        text.contains("3.000 s to 4.000 s: red (failed) - nozzle 1: carrying"),
        "{text}"
    );
    let csv = tower::csv(&t);
    assert!(csv.starts_with("from_s,to_s,light,why,nozzle,nozzle_state\n"));
    assert!(csv.contains("2.000,3.000,red,failed,2,idle"), "{csv}");
    assert!(csv.contains("0.000,2.000,green,running,0,"), "{csv}");
}

#[test]
fn a_journal_without_the_column_gives_the_report_it_always_gave() {
    let old = "step,at_s,took_s,action,reference,feeder,nozzle,result,status\n1,0,1,pick,R1,F1,1,ok,pending\n2,1,1,place,R1,F1,1,PHOENIX-PNP-024,pending\n";
    let t = tower::timeline(&read_journal(old).unwrap(), 2.0, 3.0);
    let text = tower::text(&t);
    assert!(
        text.contains("1.000 s to 2.000 s: red (failed)\n"),
        "{text}"
    );
    assert!(t.iter().all(|i| i.nozzle == 0));
}

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

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn sim_tower_names_the_nozzle_of_the_failed_step_and_ends_with_2() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_nv_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("journal.csv");
    std::fs::write(&file, journal(&FAILING)).unwrap();
    let o = run(&["sim-tower", p(&file)]);
    let text = String::from_utf8_lossy(&o.stdout).into_owned();
    assert_eq!(o.status.code(), Some(2), "{text}");
    assert!(text.contains("red (failed) - nozzle 2: idle"), "{text}");
    // a journal that the simulation wrote carries the column, and the tower reads it
    let jour = dir.join("sim.csv");
    let o = run(&[
        "simulate",
        "examples/board.pos",
        "examples/feeders.csv",
        "--journal",
        p(&jour),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let written = std::fs::read_to_string(&jour).unwrap();
    assert!(
        written.lines().next().unwrap().ends_with(",nozzle_state"),
        "{written}"
    );
    let steps = read_journal(&written).unwrap();
    assert!(!steps.is_empty() && steps.iter().all(|s| !s.nozzle_state.is_empty()));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the Job tab

fn app(cfg: &str) -> AppState {
    let mut a = AppState::new(UiSettings::default());
    let mut files = vec![("machine.cfg".to_string(), cfg.to_string())];
    files.extend(samples::mini());
    assert_eq!(a.load_sample(&files), 3);
    a
}

fn rows_of(a: &AppState) -> Vec<JobRow> {
    job_rows(
        a.planned.as_ref().unwrap(),
        &a.ws.dnp,
        Side::Top,
        a.session.job(),
    )
}

#[test]
fn the_job_tab_says_what_every_nozzle_is_doing_between_two_steps() {
    let mut a = app("");
    // no job yet: nothing to say
    assert!(nozzles_now(a.session.job()).is_empty());
    assert!(nozzle_line(a.session.job(), |t| tr(t, Lang::En)).is_empty());
    assert!(rows_of(&a).iter().all(|r| r.nozzle_state.is_none()));
    a.step();
    let now = nozzles_now(a.session.job());
    assert_eq!(now.len(), 1, "{now:?}");
    assert_eq!(now[0].state, NozzleState::Carrying);
    assert!(!now[0].part.is_empty());
    let line = nozzle_line(a.session.job(), |t| tr(t, Lang::En));
    assert_eq!(line, format!("1: carrying a part ({})", now[0].part));
    let pt = nozzle_line(a.session.job(), |t| tr(t, Lang::Pt));
    assert!(pt.contains("com uma peça"), "{pt}");
    // every row says what its own nozzle is doing
    assert!(rows_of(&a)
        .iter()
        .filter(|r| r.row.nozzle > 0)
        .all(|r| r.nozzle_state == Some(NozzleState::Carrying)));
    // the part is placed: the nozzle is idle again
    a.step();
    let after = nozzles_now(a.session.job());
    assert!(
        after
            .iter()
            .all(|n| n.state == NozzleState::Idle && n.part.is_empty()),
        "{after:?}"
    );
    assert_eq!(nozzle_line(a.session.job(), |t| tr(t, Lang::En)), "1: idle");
    assert!(rows_of(&a)
        .iter()
        .filter(|r| r.row.nozzle > 0)
        .all(|r| r.nozzle_state == Some(NozzleState::Idle)));
}

#[test]
fn the_four_states_have_a_word_in_each_language() {
    for lang in Lang::ALL {
        let words: Vec<&str> = NozzleState::ALL
            .iter()
            .map(|s| tr(s.text(), lang))
            .collect();
        assert!(words.iter().all(|w| !w.is_empty()), "{lang:?}");
        let distinct: std::collections::BTreeSet<_> = words.iter().collect();
        assert_eq!(distinct.len(), 4, "{lang:?}: {words:?}");
    }
    assert_eq!(tr(Text::NozzleIdle, Lang::En), "idle");
}
