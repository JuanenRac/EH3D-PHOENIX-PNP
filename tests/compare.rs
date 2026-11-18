//! The comparison of the orders of the places and of the ways of changing the tips: the plans side by side, the fastest marked, the advice and the command `order-compare`.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::compare::{
    compare, fastest, tip_strategy_name, Comparison, Row, ORDERS, TIP_STRATEGIES,
};
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::parts::{parse, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use std::process::{Command, Output};

const BOARD: &str = include_str!("../examples/board.pos");
const FEEDERS: &str = include_str!("../examples/feeders.csv");
const PACKAGES: &str = include_str!("../examples/packages.csv");

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

fn row(name: &'static str, travel_mm: f64, seconds: f64) -> Row {
    Row {
        name,
        batches: 1,
        tip_changes: 0,
        travel_mm,
        seconds,
    }
}

fn comparison_of(cfg_text: &str) -> (Comparison, MachineConfig) {
    let cfg = MachineConfig::parse(cfg_text).unwrap();
    let parts = parse(BOARD).unwrap();
    let feeders = feeding::from_csv(FEEDERS).unwrap();
    let picks = assign(&parts, &feeders, Some(Side::Top));
    let mut catalog = Catalog::new();
    catalog.add_packages(PACKAGES).unwrap();
    let c = compare(
        &parts,
        &feeders,
        &picks,
        &catalog,
        &cfg,
        &Options::default(),
    )
    .unwrap();
    (c, cfg)
}

#[test]
fn the_fastest_has_the_fewest_seconds_then_the_shortest_travel_then_comes_first() {
    assert_eq!(fastest(&[]), None);
    assert_eq!(fastest(&[row("a", 5.0, 9.0)]), Some(0));
    let rows = [
        row("a", 50.0, 10.0),
        row("b", 90.0, 8.0),
        row("c", 70.0, 8.0),
        row("d", 70.0, 8.0),
        row("e", 10.0, 12.0),
    ];
    // b and c have the fewest seconds, c travels less; d is the same as c and comes later
    assert_eq!(fastest(&rows), Some(2));
}

#[test]
fn every_order_and_every_way_of_changing_the_tips_is_planned_as_the_configuration_would() {
    let (c, _) = comparison_of("tip_changer_x_mm = -50\ntip_changer_y_mm = 0");
    let order_names: Vec<&str> = c.orders.iter().map(|r| r.name).collect();
    assert_eq!(
        order_names,
        ["nearest", "part", "height_part", "pick", "tips", "none"]
    );
    let tip_names: Vec<&str> = c.tips.iter().map(|r| r.name).collect();
    assert_eq!(tip_names, ["minimize", "as_planned", "optimize"]);
    assert_eq!((ORDERS.len(), TIP_STRATEGIES.len()), (6, 3));
    assert_eq!(tip_strategy_name(TIP_STRATEGIES[1]), "as_planned");
    assert_eq!((c.order_now, c.tips_now), ("nearest", "minimize"));
    // a row is the plan the configuration with that setting would give
    let parts = parse(BOARD).unwrap();
    let feeders = feeding::from_csv(FEEDERS).unwrap();
    let picks = assign(&parts, &feeders, Some(Side::Top));
    let mut catalog = Catalog::new();
    catalog.add_packages(PACKAGES).unwrap();
    for r in &c.orders {
        let cfg = MachineConfig::parse(&format!(
            "tip_changer_x_mm = -50\ntip_changer_y_mm = 0\norder = {}",
            r.name
        ))
        .unwrap();
        let plan = plan_advanced(
            &parts,
            &feeders,
            &picks,
            &catalog,
            &cfg,
            &Options::default(),
        )
        .unwrap();
        assert_eq!(
            (r.batches, r.tip_changes, r.travel_mm, r.seconds),
            (
                plan.plan.batches,
                plan.tip_changes,
                plan.plan.travel_mm,
                plan.plan.seconds
            ),
            "{}",
            r.name
        );
    }
    assert!(c
        .orders
        .iter()
        .all(|r| r.seconds > 0.0 && r.travel_mm > 0.0));
    // the comparison starts from the configuration it is given
    let (other, _) = comparison_of("order = part\ntip_strategy = as_planned");
    assert_eq!((other.order_now, other.tips_now), ("part", "as_planned"));
}

#[test]
fn the_text_marks_the_configuration_and_the_fastest() {
    let (c, _) = comparison_of("");
    let t = c.text();
    assert!(t.starts_with("order of the places\n"), "{t}");
    assert!(t.contains("change of the nozzle tips\n"), "{t}");
    assert_eq!(t.matches("<- fastest").count(), 2, "{t}");
    assert_eq!(t.matches("<- the configuration").count(), 2, "{t}");
    let nearest = t
        .lines()
        .find(|l| l.trim_start().starts_with("nearest"))
        .unwrap();
    assert!(nearest.contains("<- the configuration"), "{nearest}");
    // a title and a header, and a line for each setting, for each of the two lists
    assert_eq!(t.lines().count(), 2 + 6 + 2 + 3);
}

#[test]
fn the_advice_says_what_a_better_setting_would_save_or_that_the_configuration_is_the_fastest() {
    let mut c = Comparison {
        orders: vec![row("nearest", 100.0, 10.0), row("part", 80.0, 7.5)],
        tips: vec![row("minimize", 100.0, 10.0), row("optimize", 120.0, 10.0)],
        order_now: "nearest",
        tips_now: "minimize",
    };
    assert_eq!(c.seconds_saved_by_order(), 2.5);
    // a tie in the seconds with more travel is no better
    assert_eq!(c.seconds_saved_by_tips(), 0.0);
    assert_eq!(
        c.advice(),
        "order = part saves 2.5 s over order = nearest\ntip_strategy = minimize is already the fastest\n"
    );
    c.order_now = "part";
    assert_eq!(c.seconds_saved_by_order(), 0.0);
    assert!(c
        .advice()
        .starts_with("order = part is already the fastest\n"));
    // a setting that is not in the list saves nothing, and no rows say nothing
    c.order_now = "unknown";
    assert_eq!(c.seconds_saved_by_order(), 0.0);
    let empty = Comparison {
        orders: vec![],
        tips: vec![],
        order_now: "nearest",
        tips_now: "minimize",
    };
    assert_eq!(empty.advice(), "");
    assert_eq!(empty.text().matches("<- fastest").count(), 0);
}

#[test]
fn the_command_prints_both_lists_and_the_advice() {
    let o = run(&[
        "order-compare",
        "examples/board.pos",
        "examples/feeders.csv",
        "--packages",
        "examples/packages.csv",
        "--config",
        "examples/machine.cfg",
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    assert!(out.contains("order of the places"), "{out}");
    assert!(out.contains("tip_strategy"), "{out}");
    assert!(out.contains("<- fastest"), "{out}");
    assert!(
        out.contains("order = ") && out.contains("tip_strategy = "),
        "{out}"
    );
    // the option of the command moves the mark of the configuration
    let o = run(&[
        "order-compare",
        "examples/board.pos",
        "examples/feeders.csv",
        "--order",
        "part",
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    let line = out
        .lines()
        .find(|l| l.trim_start().starts_with("part "))
        .unwrap();
    assert!(line.contains("<- the configuration"), "{out}");
    // a plan that cannot be made is the error of the command
    let o = run(&[
        "order-compare",
        "examples/board.pos",
        "examples/feeders.csv",
        "--side",
        "bottom",
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        text(&o.stderr).contains("PHOENIX-PNP-017"),
        "{}",
        text(&o.stderr)
    );
    let o = run(&["order-compare", "examples/board.pos"]);
    assert_eq!(o.status.code(), Some(1));
}
