//! The checks before a job: what they find, how severe it is and what it points at.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use eh3d_phoenix_pnp::preflight::{has_errors, preflight, to_text, Check, Severity, Source};

const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nC1,100n,C,10,20,0,top\nU1,MCU,Q,40,15,0,top\nD1,LED,L,5,5,0,bottom\n";
const FEEDERS: &str = "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\nF4,LED,L,-20,50,5\n";
const TABLES: &str = "package\nR\nC\nQ\nL\n";

fn checks_of(parts: &str, feeders: &str, tables: &str, cfg: &str, side: Side) -> Vec<Check> {
    let cfg = MachineConfig::parse(cfg).unwrap();
    let parts = from_csv(parts).unwrap();
    let feeders = feeding::from_csv(feeders).unwrap();
    let picks = assign(&parts, &feeders, Some(side));
    let mut catalog = Catalog::new();
    if !tables.is_empty() {
        catalog.add_packages(tables).unwrap();
    }
    let opts = Options {
        side,
        board_width_mm: Some(100.0),
        origin_x_mm: 0.0,
    };
    let plan = plan_advanced(&parts, &feeders, &picks, &catalog, &cfg, &opts).unwrap();
    preflight(&parts, side, &picks, &catalog, &cfg, Some(&plan))
}

fn find<'a>(checks: &'a [Check], source: &Source) -> Vec<&'a Check> {
    checks.iter().filter(|c| &c.source == source).collect()
}

#[test]
fn a_board_that_is_ready_has_nothing_to_report() {
    let c = checks_of(PARTS, FEEDERS, TABLES, "", Side::Top);
    assert!(c.is_empty(), "{}", to_text(&c));
    assert!(!has_errors(&c));
    assert_eq!(to_text(&c), "");
}

#[test]
fn a_part_without_a_feeder_and_a_feeder_that_is_short_are_errors() {
    let parts = format!("{PARTS}X1,odd,Z,1,1,0,top\n");
    let feeders = FEEDERS.replace("F3,MCU,Q,-20,40,2", "F3,MCU,Q,-20,40,0");
    let c = checks_of(&parts, &feeders, TABLES, "", Side::Top);
    assert!(has_errors(&c));
    let x1 = find(&c, &Source::Part("X1".to_string()));
    assert_eq!(x1.len(), 1);
    assert_eq!(x1[0].severity, Severity::Error);
    assert_eq!(x1[0].message, "no feeder holds this part");
    let f3 = find(&c, &Source::Feeder("F3".to_string()));
    assert_eq!(f3.len(), 1);
    assert_eq!(f3[0].severity, Severity::Error);
    assert!(f3[0].message.contains("short of 1 parts"));
    // the text names what it is about
    let text = to_text(&c);
    assert!(
        text.contains("error: part X1: no feeder holds this part\n"),
        "{text}"
    );
    assert!(text.contains("error: feeder F3: "), "{text}");
}

#[test]
fn a_feeder_that_will_be_nearly_empty_and_kinds_that_no_table_describes_are_warnings() {
    let feeders = FEEDERS.replace(
        "id,value,package,x,y,count\nF1,10k,R,-20,10,10",
        "id,value,package,x,y,count,warn_below\nF1,10k,R,-20,10,3",
    );
    // the other rows need the extra column
    let feeders = feeders
        .lines()
        .enumerate()
        .map(|(i, l)| {
            if i < 2 {
                l.to_string()
            } else {
                format!("{l},")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let feeders = feeders.replace("F1,10k,R,-20,10,3", "F1,10k,R,-20,10,3,2");
    let c = checks_of(PARTS, &feeders, "package\nC\nQ\nL\n", "", Side::Top);
    let f1 = find(&c, &Source::Feeder("F1".to_string()));
    assert_eq!(f1.len(), 1, "{}", to_text(&c));
    assert_eq!(f1[0].severity, Severity::Warning);
    assert!(f1[0].message.contains("only 1 parts will be left"));
    let r = find(&c, &Source::Part("10k / R".to_string()));
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].severity, Severity::Warning);
    assert!(r[0].message.contains("2 parts of this kind"));
    assert!(!has_errors(&c));
}

#[test]
fn a_part_too_tall_or_too_deep_for_the_travel_of_z_is_an_error() {
    let tall = checks_of(
        PARTS,
        FEEDERS,
        "package,height_mm\nR,60\nC,\nQ,\nL,\n",
        "",
        Side::Top,
    );
    let r = find(&tall, &Source::Part("10k / R".to_string()));
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].severity, Severity::Error);
    assert!(r[0].message.contains("above the travel of Z"));
    let deep = checks_of(
        PARTS,
        FEEDERS,
        "package,through_board_mm\nR,\nC,15\nQ,\nL,\n",
        "",
        Side::Top,
    );
    let c = find(&deep, &Source::Part("100n / C".to_string()));
    assert_eq!(c.len(), 1);
    assert!(c[0].message.contains("below the travel of Z"));
    // raising the top of the travel lets the tall part through
    let fine = checks_of(
        PARTS,
        FEEDERS,
        "package,height_mm\nR,60\nC,\nQ,\nL,\n",
        "z_max_mm = 100",
        Side::Top,
    );
    assert!(fine.is_empty(), "{}", to_text(&fine));
}

#[test]
fn a_place_outside_the_limits_of_the_machine_is_an_error_for_that_part() {
    let c = checks_of(PARTS, FEEDERS, TABLES, "limit_x_max_mm = 15", Side::Top);
    for r in ["R2", "U1"] {
        let found = find(&c, &Source::Part(r.to_string()));
        assert_eq!(found.len(), 1, "{r}");
        assert_eq!(found[0].severity, Severity::Error);
    }
    assert!(find(&c, &Source::Part("R1".to_string())).is_empty());
    // a feeder outside the limits is found through the pick point
    let c = checks_of(PARTS, FEEDERS, TABLES, "limit_x_min_mm = -10", Side::Top);
    assert!(has_errors(&c));
}

#[test]
fn several_nozzle_tips_need_the_place_of_the_changer() {
    let tables = "package,nozzle_tip\nR,N1\nC,N2\nQ,N1\nL,N1\n";
    let without = checks_of(PARTS, FEEDERS, tables, "", Side::Top);
    let setting = Source::Setting("tip_changer_x_mm".to_string());
    let w = find(&without, &setting);
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].severity, Severity::Warning);
    let with = checks_of(
        PARTS,
        FEEDERS,
        tables,
        "tip_changer_x_mm = -50\ntip_changer_y_mm = 0",
        Side::Top,
    );
    assert!(find(&with, &setting).is_empty());
}

#[test]
fn groups_of_ranks_and_a_feeder_that_is_never_switched_off_are_information() {
    let c = checks_of(
        PARTS,
        FEEDERS,
        "package,rank\nR,0\nC,10\nQ,0\nL,0\n",
        "fault_limit = 0",
        Side::Top,
    );
    let rank = find(&c, &Source::Setting("rank_step".to_string()));
    assert_eq!(rank.len(), 1);
    assert_eq!(rank[0].severity, Severity::Info);
    let fault = find(&c, &Source::Setting("fault_limit".to_string()));
    assert_eq!(fault.len(), 1);
    assert_eq!(fault[0].severity, Severity::Info);
    assert!(!has_errors(&c));
}

#[test]
fn a_side_without_parts_is_a_warning_and_the_other_side_is_checked_by_itself() {
    let top_only = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\n";
    let c = checks_of(top_only, FEEDERS, TABLES, "", Side::Bottom);
    let board = find(&c, &Source::Board);
    assert_eq!(board.len(), 1);
    assert_eq!(board[0].severity, Severity::Warning);
    // the bottom side of the board: its parts have feeders, and the top parts are not asked for any
    let bottom = checks_of(PARTS, FEEDERS, TABLES, "", Side::Bottom);
    assert!(bottom.is_empty(), "{}", to_text(&bottom));
}

#[test]
fn the_findings_come_in_a_fixed_order_with_the_errors_first() {
    let parts = format!("{PARTS}X1,odd,Z,1,1,0,top\n");
    let c = checks_of(
        &parts,
        FEEDERS,
        "package,rank\nR,0\nC,10\n",
        "fault_limit = 0\nlimit_x_max_mm = 15",
        Side::Top,
    );
    assert!(c.len() >= 4);
    let severities: Vec<Severity> = c.iter().map(|x| x.severity).collect();
    let mut sorted = severities.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(severities, sorted, "{}", to_text(&c));
    let again = checks_of(
        &parts,
        FEEDERS,
        "package,rank\nR,0\nC,10\n",
        "fault_limit = 0\nlimit_x_max_mm = 15",
        Side::Top,
    );
    assert_eq!(c, again);
}
