//! The time of the moves, the shortening of the walk, and the planner that knows the configuration: ranks, order strategies, nozzle tips, the limits of the nozzle and the origin of a part.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::{Code, PnpError};
use eh3d_phoenix_pnp::feeders::Feeder;
use eh3d_phoenix_pnp::feeding::{assign, FeederSpec};
use eh3d_phoenix_pnp::motion::{move_time, AxisMotion};
use eh3d_phoenix_pnp::parts::{Part, Side};
use eh3d_phoenix_pnp::planner::{path_length, plan_advanced, two_opt, AdvancedPlan, Options};

struct Rng(u64);

impl Rng {
    fn bits(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.bits() % n as u64) as usize
    }

    fn between(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * ((self.bits() >> 11) as f64 / (1u64 << 53) as f64)
    }

    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }
}

fn part(r: &str, v: &str, p: &str, x: f64, y: f64, rot: f64) -> Part {
    Part {
        reference: r.into(),
        value: v.into(),
        package: p.into(),
        x,
        y,
        rotation: rot,
        side: Side::Top,
    }
}

fn feeder(id: &str, v: &str, p: &str, x: f64, y: f64, count: u32) -> FeederSpec {
    FeederSpec::from_basic(Feeder {
        id: id.into(),
        value: v.into(),
        package: p.into(),
        x,
        y,
        count,
    })
}

fn make(
    parts: &[Part],
    feeders: &[FeederSpec],
    packages: &str,
    cfg: &str,
    opts: Options,
) -> Result<AdvancedPlan, PnpError> {
    let cfg = MachineConfig::parse(cfg).unwrap();
    let mut catalog = Catalog::new();
    if !packages.is_empty() {
        catalog.add_packages(packages).unwrap();
    }
    let picks = assign(parts, feeders, Some(opts.side));
    plan_advanced(parts, feeders, &picks, &catalog, &cfg, &opts)
}

fn letters(plan: &AdvancedPlan) -> String {
    plan.plan
        .placements
        .iter()
        .map(|p| p.reference.chars().next().unwrap())
        .collect()
}

fn refs(plan: &AdvancedPlan) -> Vec<&str> {
    plan.plan
        .placements
        .iter()
        .map(|p| p.reference.as_str())
        .collect()
}

// ---- the time of a move

#[test]
fn a_move_takes_as_long_as_the_profile_says() {
    let m = AxisMotion::new(150.0, 2000.0, 0.0).unwrap();
    // 100 mm: the top speed is reached after 11.25 mm; 100 / 150 + 150 / 2000
    assert!((m.time(100.0) - (100.0 / 150.0 + 0.075)).abs() < 1e-9);
    // 5 mm: the top speed is never reached; the peak speed is sqrt(5 * 2000) and the time 2 * peak / acceleration
    assert!((m.time(5.0) - 0.1).abs() < 1e-9);
    assert_eq!(m.time(0.0), 0.0);
    assert_eq!(m.time(-100.0), m.time(100.0));
    let s = AxisMotion::new(150.0, 2000.0, 50_000.0).unwrap();
    assert!((s.time(100.0) - (100.0 / 150.0 + 0.115)).abs() < 1e-9);
}

#[test]
fn a_longer_move_never_takes_less_time_and_a_jerk_limit_never_makes_it_faster() {
    let mut rng = Rng(21);
    let flat = AxisMotion::new(200.0, 3000.0, 0.0).unwrap();
    let smooth = AxisMotion::new(200.0, 3000.0, 80_000.0).unwrap();
    for _ in 0..300 {
        let (a, b) = (rng.between(0.0, 300.0), rng.between(0.0, 300.0));
        let (short, long) = if a < b { (a, b) } else { (b, a) };
        assert!(flat.time(short) <= flat.time(long) + 1e-12);
        assert!(smooth.time(short) <= smooth.time(long) + 1e-12);
        assert!(
            flat.time(long) >= long / 200.0 - 1e-12,
            "faster than the top speed"
        );
        assert!(smooth.time(long) >= flat.time(long) - 1e-9);
    }
    // there is no jump where the top speed starts to be reached
    let edge = 200.0 * (200.0 / 3000.0);
    assert!((flat.time(edge - 1e-7) - flat.time(edge + 1e-7)).abs() < 1e-5);
}

#[test]
fn two_axes_move_together_and_a_bad_motion_is_refused() {
    let x = AxisMotion::new(100.0, 1000.0, 0.0).unwrap();
    let y = AxisMotion::new(300.0, 5000.0, 0.0).unwrap();
    assert_eq!(move_time(50.0, 5.0, &x, &y), x.time(50.0));
    assert_eq!(move_time(5.0, 80.0, &x, &y), y.time(80.0));
    assert_eq!(move_time(0.0, 0.0, &x, &y), 0.0);
    for (v, a, j) in [
        (0.0, 1.0, 0.0),
        (1.0, 0.0, 0.0),
        (1.0, 1.0, -1.0),
        (f64::NAN, 1.0, 0.0),
    ] {
        assert_eq!(
            AxisMotion::new(v, a, j).unwrap_err().code,
            Code::PlanSetting
        );
    }
}

// ---- the 2-opt moves

#[test]
fn two_opt_untangles_a_walk() {
    let points = vec![(10.0, 0.0), (1.0, 0.0), (2.0, 0.0), (3.0, 0.0)];
    let mut order = vec![0, 1, 2, 3];
    let before = path_length(&points, &order, (0.0, 0.0));
    assert!((before - 21.0).abs() < 1e-9);
    two_opt(&points, &mut order, (0.0, 0.0), 10);
    assert!((path_length(&points, &order, (0.0, 0.0)) - 10.0).abs() < 1e-9);
    assert_eq!(order, vec![1, 2, 3, 0]);
    // nothing to do for one point or none
    let mut one = vec![0];
    two_opt(&points, &mut one, (0.0, 0.0), 10);
    assert_eq!(one, vec![0]);
    let mut none: Vec<usize> = Vec::new();
    two_opt(&points, &mut none, (0.0, 0.0), 10);
    assert!(none.is_empty());
}

#[test]
fn two_opt_never_makes_the_walk_longer_and_keeps_every_point() {
    let mut rng = Rng(33);
    for _ in 0..100 {
        let n = 2 + rng.below(40);
        let points: Vec<(f64, f64)> = (0..n)
            .map(|_| (rng.between(0.0, 100.0), rng.between(0.0, 100.0)))
            .collect();
        let mut order: Vec<usize> = (0..n).collect();
        rng.shuffle(&mut order);
        let start = (rng.between(0.0, 100.0), rng.between(0.0, 100.0));
        let before = path_length(&points, &order, start);
        let mut after_order = order.clone();
        two_opt(&points, &mut after_order, start, 1000);
        assert!(path_length(&points, &after_order, start) <= before + 1e-9);
        let mut sorted = after_order.clone();
        sorted.sort_unstable();
        assert_eq!(
            sorted,
            (0..n).collect::<Vec<_>>(),
            "a point was lost or repeated"
        );
        // a second run finds nothing more
        let mut again = after_order.clone();
        two_opt(&points, &mut again, start, 1000);
        assert_eq!(again, after_order);
    }
}

// ---- the planner

#[test]
fn the_time_of_a_plan_is_the_sum_of_what_the_model_says() {
    let parts = vec![part("R1", "10k", "R", 100.0, 0.0, 0.0)];
    let feeders = vec![feeder("F", "10k", "R", 0.0, 0.0, 5)];
    let plan = make(&parts, &feeders, "", "", Options::default()).unwrap();
    // the feed (0.2) and the pick (0.4) at the feeder, the move of 100 mm, the place (0.4)
    let expected = 0.2 + 0.4 + (100.0 / 150.0 + 0.075) + 0.4;
    assert!(
        (plan.plan.seconds - expected).abs() < 1e-9,
        "{}",
        plan.plan.seconds
    );
    assert!((plan.plan.travel_mm - 100.0).abs() < 1e-9);
    assert_eq!(plan.plan.batches, 1);
    assert_eq!(plan.tip_changes, 0);
    assert_eq!(plan.rotation_s, 0.0);
    assert_eq!(plan.details[0].rank, 0);
}

#[test]
fn a_hard_order_between_blocks_of_ranks_and_a_preference_inside_a_block() {
    let parts = vec![
        part("A1", "va", "A", 100.0, 0.0, 0.0),
        part("B1", "vb", "B", 1.0, 0.0, 0.0),
    ];
    let feeders = vec![
        feeder("FA", "va", "A", 0.0, 0.0, 5),
        feeder("FB", "vb", "B", 0.0, 5.0, 5),
    ];
    let cfg = "nozzles = 2";
    // ranks 0 and 10: two blocks, the far part first, never in the same batch
    let hard = make(
        &parts,
        &feeders,
        "package,rank\nA,0\nB,10\n",
        cfg,
        Options::default(),
    )
    .unwrap();
    assert_eq!(refs(&hard), vec!["A1", "B1"]);
    assert_eq!(hard.plan.batches, 2);
    assert_eq!(hard.tiers, 2);
    // ranks 0 and 5: one block, the lower rank first, and a batch may take both
    let weak = make(
        &parts,
        &feeders,
        "package,rank\nA,0\nB,5\n",
        cfg,
        Options::default(),
    )
    .unwrap();
    assert_eq!(refs(&weak), vec!["A1", "B1"]);
    assert_eq!(weak.plan.batches, 1);
    assert_eq!(weak.tiers, 1);
    // the rank decides, not the distance
    let flipped = make(
        &parts,
        &feeders,
        "package,rank\nA,10\nB,0\n",
        cfg,
        Options::default(),
    )
    .unwrap();
    assert_eq!(refs(&flipped), vec!["B1", "A1"]);
    // the blocks are the tens: 9 and 10 are two blocks, and so are -1 and 0
    for table in ["package,rank\nA,9\nB,10\n", "package,rank\nA,-1\nB,0\n"] {
        let p = make(&parts, &feeders, table, cfg, Options::default()).unwrap();
        assert_eq!(p.tiers, 2, "{table}");
        assert_eq!(p.plan.batches, 2, "{table}");
    }
    // the step of the blocks is in the configuration
    let wide = make(
        &parts,
        &feeders,
        "package,rank\nA,0\nB,10\n",
        "nozzles = 2\nrank_step = 100",
        Options::default(),
    )
    .unwrap();
    assert_eq!(wide.tiers, 1);
}

/// Six parts on a line: X Y Z X Y Z, one millimetre apart, each kind with its own feeder, height and nozzle tip.
fn line_board() -> (Vec<Part>, Vec<FeederSpec>, &'static str) {
    let parts = vec![
        part("X1", "x", "PX", 0.0, 0.0, 0.0),
        part("Y1", "y", "PY", 1.0, 0.0, 0.0),
        part("Z1", "z", "PZ", 2.0, 0.0, 0.0),
        part("X2", "x", "PX", 3.0, 0.0, 0.0),
        part("Y2", "y", "PY", 4.0, 0.0, 0.0),
        part("Z2", "z", "PZ", 5.0, 0.0, 0.0),
    ];
    let feeders = vec![
        feeder("F3", "x", "PX", -10.0, 0.0, 9),
        feeder("F1", "y", "PY", -10.0, 10.0, 9),
        feeder("F2", "z", "PZ", -10.0, 20.0, 9),
    ];
    (
        parts,
        feeders,
        "package,height_mm,nozzle_tip\nPX,1.0,N2\nPY,0.5,N1\nPZ,2.0,N2\n",
    )
}

#[test]
fn the_strategies_of_order_group_the_parts_in_their_own_way() {
    let (parts, feeders, tables) = line_board();
    let order = |name: &str| {
        let cfg = format!("tip_strategy = as_planned\norder = {name}");
        letters(&make(&parts, &feeders, tables, &cfg, Options::default()).unwrap())
    };
    assert_eq!(order("nearest"), "XYZXYZ");
    assert_eq!(order("part"), "XXYYZZ");
    assert_eq!(order("height_part"), "YYXXZZ");
    // the feeders are F3 for X, F1 for Y and F2 for Z, and are visited in the order of their ids
    assert_eq!(order("pick"), "YYZZXX");
    // the tip N1 (Y) comes before N2 (X and Z)
    let tips = order("tips");
    assert!(tips.starts_with("YY"), "{tips}");
    assert_eq!(tips.len(), 6);
    // no order: by reference
    let none_cfg = "order = none\ntip_strategy = as_planned";
    let none = make(&parts, &feeders, tables, none_cfg, Options::default()).unwrap();
    assert_eq!(refs(&none), vec!["X1", "X2", "Y1", "Y2", "Z1", "Z2"]);
}

#[test]
fn bringing_the_nozzle_tips_together_saves_changes_and_batches() {
    let (parts, feeders, tables) = line_board();
    let run = |cfg: &str| make(&parts, &feeders, tables, cfg, Options::default()).unwrap();
    let planned = run("tip_strategy = as_planned");
    assert_eq!(planned.tip_changes, 4);
    let minimal = run("tip_strategy = minimize");
    assert_eq!(minimal.tip_changes, 1);
    // with two nozzles a batch holds only parts of one tip
    let planned2 = run("tip_strategy = as_planned\nnozzles = 2");
    let minimal2 = run("tip_strategy = minimize\nnozzles = 2");
    assert_eq!(planned2.plan.batches, 5);
    assert_eq!(minimal2.plan.batches, 3);
    for plan in [&planned2, &minimal2] {
        for b in 0..plan.plan.batches {
            let tips: std::collections::BTreeSet<&str> = plan
                .plan
                .placements
                .iter()
                .zip(&plan.details)
                .filter(|(p, _)| p.batch == b)
                .map(|(_, d)| d.tip.as_str())
                .collect();
            assert_eq!(tips.len(), 1, "batch {b} mixes tips");
        }
    }
}

#[test]
fn the_tip_changer_costs_a_trip_and_the_time_of_the_change() {
    let (parts, feeders, tables) = line_board();
    let base = "tip_strategy = as_planned\ntip_change_s = 3";
    let without = make(&parts, &feeders, tables, base, Options::default()).unwrap();
    let with = make(
        &parts,
        &feeders,
        tables,
        &format!("{base}\ntip_changer_x_mm = -100\ntip_changer_y_mm = 100"),
        Options::default(),
    )
    .unwrap();
    assert_eq!(without.tip_changes, 4);
    assert!(with.plan.travel_mm > without.plan.travel_mm + 4.0 * 100.0);
    assert!(with.plan.seconds > without.plan.seconds);
    // the change itself takes the same time in both
    let none = make(
        &parts,
        &feeders,
        tables,
        "tip_strategy = minimize\ntip_change_s = 0",
        Options::default(),
    )
    .unwrap();
    let some = make(
        &parts,
        &feeders,
        tables,
        "tip_strategy = minimize\ntip_change_s = 10",
        Options::default(),
    )
    .unwrap();
    assert!((some.plan.seconds - none.plan.seconds - 10.0).abs() < 1e-9);
}

#[test]
fn a_batch_holds_as_many_parts_as_there_are_nozzles() {
    let parts: Vec<Part> = (0..5)
        .map(|i| part(&format!("R{i}"), "10k", "R", f64::from(i) * 10.0, 0.0, 0.0))
        .collect();
    let feeders = vec![feeder("F", "10k", "R", -10.0, 0.0, 9)];
    let plan = make(&parts, &feeders, "", "nozzles = 2", Options::default()).unwrap();
    assert_eq!(plan.plan.batches, 3);
    let sizes: Vec<usize> = (0..3)
        .map(|b| plan.plan.placements.iter().filter(|p| p.batch == b).count())
        .collect();
    assert_eq!(sizes, vec![2, 2, 1]);
    assert!(plan.plan.placements.iter().all(|p| p.nozzle < 2));
    assert_eq!(plan.plan.placements.len(), 5);
}

#[test]
fn a_nozzle_that_cannot_turn_far_enough_stops_the_plan() {
    let parts = vec![part("R1", "10k", "R", 10.0, 0.0, 90.0)];
    let feeders = vec![feeder("F", "10k", "R", 0.0, 0.0, 5)];
    let e = make(
        &parts,
        &feeders,
        "",
        "nozzle_angle_min_deg = 0\nnozzle_angle_max_deg = 0",
        Options::default(),
    )
    .unwrap_err();
    assert_eq!(e.code, Code::RotationUnreachable);
    // with the range of the default, it can
    assert!(make(&parts, &feeders, "", "", Options::default()).is_ok());
}

#[test]
fn turning_while_the_head_moves_saves_the_time_of_the_turn() {
    let parts = vec![part("R1", "10k", "R", 10.0, 0.0, 180.0)];
    let feeders = vec![feeder("F", "10k", "R", 0.0, 0.0, 5)];
    let early = make(&parts, &feeders, "", "pre_rotate = yes", Options::default()).unwrap();
    let late = make(&parts, &feeders, "", "pre_rotate = no", Options::default()).unwrap();
    // 10 mm take 2 * sqrt(10 * 2000) / 2000 = 0.1414 s; the half turn takes 180 / 720 = 0.25 s
    let moving = 2.0 * (10.0f64 * 2000.0).sqrt() / 2000.0;
    assert!((early.plan.seconds - late.plan.seconds + moving).abs() < 1e-9);
    assert_eq!(early.rotation_s, 0.0);
    assert!((late.rotation_s - 0.25).abs() < 1e-9);
}

#[test]
fn the_bottom_side_is_mirrored_and_needs_the_width_of_the_board() {
    let mut bottom = part("D1", "led", "L", 10.0, 5.0, 30.0);
    bottom.side = Side::Bottom;
    let parts = vec![bottom];
    let feeders = vec![feeder("F", "led", "L", 0.0, 0.0, 5)];
    let side = Options {
        side: Side::Bottom,
        ..Options::default()
    };
    assert_eq!(
        make(&parts, &feeders, "", "", side).unwrap_err().code,
        Code::PlanSetting
    );
    let ok = make(
        &parts,
        &feeders,
        "",
        "",
        Options {
            board_width_mm: Some(100.0),
            origin_x_mm: 20.0,
            ..side
        },
    )
    .unwrap();
    let p = &ok.plan.placements[0];
    assert_eq!(p.x, 2.0 * 20.0 + 100.0 - 10.0);
    assert_eq!(p.rotation, 150.0);
    let nan = Options {
        origin_x_mm: f64::NAN,
        ..Options::default()
    };
    assert_eq!(
        make(&parts, &feeders, "", "", nan).unwrap_err().code,
        Code::PlanSetting
    );
}

#[test]
fn the_centre_of_the_pads_moves_the_place_by_the_correction_turned_with_the_part() {
    let parts = vec![part("U1", "mcu", "Q", 10.0, 10.0, 90.0)];
    let feeders = vec![feeder("F", "mcu", "Q", 0.0, 0.0, 5)];
    let tables = "package,origin_dx_mm,origin_dy_mm\nQ,1,0\n";
    let cad = make(
        &parts,
        &feeders,
        tables,
        "component_origin = cad_origin",
        Options::default(),
    )
    .unwrap();
    assert_eq!(
        (cad.plan.placements[0].x, cad.plan.placements[0].y),
        (10.0, 10.0)
    );
    let pads = make(
        &parts,
        &feeders,
        tables,
        "component_origin = pads_center",
        Options::default(),
    )
    .unwrap();
    let p = &pads.plan.placements[0];
    assert!(
        (p.x - 10.0).abs() < 1e-9 && (p.y - 11.0).abs() < 1e-9,
        "{} {}",
        p.x,
        p.y
    );
}

#[test]
fn a_forged_pick_is_an_error_not_a_panic() {
    let parts = vec![part("R1", "10k", "R", 1.0, 0.0, 0.0)];
    let feeders = vec![feeder("F", "10k", "R", 0.0, 0.0, 5)];
    let mut picks = assign(&parts, &feeders, None);
    picks.picks[0].part = 9;
    let cfg = MachineConfig::default();
    let e = plan_advanced(
        &parts,
        &feeders,
        &picks,
        &Catalog::new(),
        &cfg,
        &Options::default(),
    )
    .unwrap_err();
    assert_eq!(e.code, Code::PlanSetting);
    let mut picks = assign(&parts, &feeders, None);
    picks.picks[0].feeder = 9;
    let e = plan_advanced(
        &parts,
        &feeders,
        &picks,
        &Catalog::new(),
        &cfg,
        &Options::default(),
    )
    .unwrap_err();
    assert_eq!(e.code, Code::PlanSetting);
}

#[test]
fn the_json_of_the_plan_has_the_new_fields() {
    let (parts, feeders, tables) = line_board();
    let plan = make(&parts, &feeders, tables, "", Options::default()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&plan.to_json()).unwrap();
    assert_eq!(v["schema_version"], 2);
    assert_eq!(v["order"], "nearest");
    assert_eq!(v["placements"].as_array().unwrap().len(), 6);
    assert!(v["placements"][0]["tip"].is_string());
    assert!(v["placements"][0]["rank"].is_i64());
    assert!(v["tip_changes"].is_u64());
}

#[test]
fn shortening_the_walk_does_not_make_the_places_longer_to_visit() {
    let mut rng = Rng(77);
    for _ in 0..15 {
        let parts: Vec<Part> = (0..40)
            .map(|i| {
                part(
                    &format!("R{i:02}"),
                    "10k",
                    "R",
                    rng.between(0.0, 100.0),
                    rng.between(0.0, 100.0),
                    0.0,
                )
            })
            .collect();
        let feeders = vec![feeder("F", "10k", "R", -10.0, 0.0, 99)];
        let length = |cfg: &str| {
            let plan = make(&parts, &feeders, "", cfg, Options::default()).unwrap();
            let points: Vec<(f64, f64)> = plan.plan.placements.iter().map(|p| (p.x, p.y)).collect();
            let order: Vec<usize> = (0..points.len()).collect();
            path_length(&points, &order, (0.0, 0.0))
        };
        assert!(length("two_opt = yes") <= length("two_opt = no") + 1e-6);
    }
}

#[test]
fn the_plan_does_not_depend_on_the_order_of_the_files_and_keeps_its_promises() {
    let mut rng = Rng(101);
    let tables = "package,rank,nozzle_tip,height_mm\nPA,0,N1,0.5\nPB,5,N2,1.0\nPC,12,N1,0.8\n";
    let orders = ["nearest", "part", "height_part", "pick", "tips", "none"];
    for round in 0..30 {
        let kinds = [("a", "PA"), ("b", "PB"), ("c", "PC")];
        let parts: Vec<Part> = (0..24)
            .map(|i| {
                let (v, p) = kinds[rng.below(3)];
                part(
                    &format!("P{i:02}"),
                    v,
                    p,
                    rng.between(0.0, 80.0),
                    rng.between(0.0, 80.0),
                    rng.between(0.0, 360.0),
                )
            })
            .collect();
        let feeders: Vec<FeederSpec> = kinds
            .iter()
            .enumerate()
            .map(|(n, (v, p))| feeder(&format!("F{n}"), v, p, -10.0, 15.0 * n as f64, 30))
            .collect();
        let cfg = format!(
            "order = {}\nnozzles = {}\ntip_strategy = {}",
            orders[rng.below(orders.len())],
            1 + rng.below(4),
            if rng.below(2) == 0 {
                "minimize"
            } else {
                "as_planned"
            }
        );
        let plan = make(&parts, &feeders, tables, &cfg, Options::default()).unwrap();
        // every part is placed once
        let mut seen: Vec<&str> = refs(&plan);
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), parts.len(), "round {round}");
        // the blocks of ranks come one after the other
        let tiers: Vec<i64> = plan.details.iter().map(|d| d.tier).collect();
        assert!(
            tiers.windows(2).all(|w| w[0] <= w[1]),
            "round {round}: {tiers:?}"
        );
        // a batch is as big as the nozzles, has one tip, and its parts are numbered from 0
        let nozzles = cfg
            .lines()
            .find_map(|l| l.strip_prefix("nozzles = "))
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap();
        for b in 0..plan.plan.batches {
            let members: Vec<usize> = (0..plan.plan.placements.len())
                .filter(|i| plan.plan.placements[*i].batch == b)
                .collect();
            assert!(
                !members.is_empty() && members.len() <= nozzles,
                "round {round}"
            );
            assert!(members
                .iter()
                .all(|i| plan.details[*i].tip == plan.details[members[0]].tip));
            assert!(members
                .iter()
                .all(|i| plan.details[*i].tier == plan.details[members[0]].tier));
        }
        // the same plan whatever the order of the files
        let (mut p2, mut f2) = (parts.clone(), feeders.clone());
        rng.shuffle(&mut p2);
        rng.shuffle(&mut f2);
        let again = make(&p2, &f2, tables, &cfg, Options::default()).unwrap();
        assert_eq!(plan.to_json(), again.to_json(), "round {round}");
    }
}
// ---- the search for the order of the nozzle tips

const TIP_TABLES: &str = "package,nozzle_tip\nPA,TA\nPB,TB\n";

#[test]
fn the_order_of_the_tips_is_searched_for_the_shortest_way() {
    // B1 is met first (40 mm), then A1 and A2 (50 and 60 mm), and B2 last (500 mm). Bringing the tips together in the order in which they appear gives B1 B2 A1 A2:
    // 40 + 460 to go through B, 450 back to A and 10 through A, 960 mm in all; A1 A2 B1 B2 is 50 + 10 + 20 + 460 = 540 mm
    let parts = [
        part("B1", "vb", "PB", 40.0, 0.0, 0.0),
        part("A1", "va", "PA", 50.0, 0.0, 0.0),
        part("A2", "va", "PA", 60.0, 0.0, 0.0),
        part("B2", "vb", "PB", 500.0, 0.0, 0.0),
    ];
    let feeders = [
        feeder("FA", "va", "PA", -20.0, 0.0, 5),
        feeder("FB", "vb", "PB", -20.0, 10.0, 5),
    ];
    let run = |extra: &str| {
        make(
            &parts,
            &feeders,
            TIP_TABLES,
            &format!("nozzles = 1\ntwo_opt = no\n{extra}"),
            Options::default(),
        )
        .unwrap()
    };
    let grouped = run("tip_strategy = minimize");
    assert_eq!(refs(&grouped), ["B1", "B2", "A1", "A2"]);
    let best = run("tip_strategy = optimize");
    assert_eq!(refs(&best), ["A1", "A2", "B1", "B2"]);
    assert_eq!(best.tip_changes, 1);
    assert_eq!(grouped.tip_changes, 1);
    // one order tried is the order of appearance: the same as grouping
    let one = run("tip_strategy = optimize\ntip_opt_max_evals = 1");
    assert_eq!(refs(&one), refs(&grouped));
    // a search that is told to stop at once (the clock is read every 256 orders, so two tips are searched whole)
    let quick = run("tip_strategy = optimize\ntip_opt_time_limit_s = 0.01");
    assert_eq!(refs(&quick), refs(&best));
}

#[test]
fn the_tip_that_is_on_the_nozzle_when_a_block_of_ranks_starts_is_used_first() {
    // R1 (tip A) is alone in the block of rank 0; the block of rank 10 has R2 (tip B) at 10 mm and R3 (tip A) at 20 mm.
    // Grouping goes B then A: A -> B -> A, two changes. Starting with A saves one: A -> A -> B
    let tables = "package,rank,nozzle_tip\nPA,0,A\nPB,10,B\nPC,10,A\n";
    let parts = [
        part("R1", "v1", "PA", 0.0, 0.0, 0.0),
        part("R2", "v2", "PB", 10.0, 0.0, 0.0),
        part("R3", "v3", "PC", 20.0, 0.0, 0.0),
    ];
    let feeders = [
        feeder("F1", "v1", "PA", -20.0, 0.0, 5),
        feeder("F2", "v2", "PB", -20.0, 10.0, 5),
        feeder("F3", "v3", "PC", -20.0, 20.0, 5),
    ];
    let run = |strategy: &str| {
        make(
            &parts,
            &feeders,
            tables,
            &format!("nozzles = 1\ntwo_opt = no\ntip_strategy = {strategy}"),
            Options::default(),
        )
        .unwrap()
    };
    let grouped = run("minimize");
    assert_eq!(refs(&grouped), ["R1", "R2", "R3"]);
    assert_eq!(grouped.tip_changes, 2);
    let best = run("optimize");
    assert_eq!(refs(&best), ["R1", "R3", "R2"]);
    assert_eq!(best.tip_changes, 1);
    // one change less is three seconds less in the time of the plan, and the walk is not longer by more than the way it takes
    assert!(best.plan.seconds < grouped.plan.seconds);
}

#[test]
fn more_than_eight_tips_are_ordered_by_the_nearest_first_and_every_part_is_placed() {
    let mut parts = Vec::new();
    let mut feeders = Vec::new();
    let mut tables = String::from("package,nozzle_tip\n");
    for k in 0..9 {
        // the tips are met from the far end to the near one
        let x = 100.0 - 10.0 * k as f64;
        parts.push(part(
            &format!("P{k}"),
            &format!("v{k}"),
            &format!("K{k}"),
            x,
            0.0,
            0.0,
        ));
        feeders.push(feeder(
            &format!("F{k}"),
            &format!("v{k}"),
            &format!("K{k}"),
            -20.0,
            k as f64,
            3,
        ));
        tables.push_str(&format!("K{k},T{k}\n"));
    }
    let plan = make(
        &parts,
        &feeders,
        &tables,
        "nozzles = 1\ntwo_opt = no\ntip_strategy = optimize",
        Options::default(),
    )
    .unwrap();
    assert_eq!(plan.plan.placements.len(), 9);
    // nine different tips: eight changes, whatever the order
    assert_eq!(plan.tip_changes, 8);
    // from the origin, the nearest part is P8 (x = 20) and the walk goes on towards the far end
    assert_eq!(
        refs(&plan),
        ["P8", "P7", "P6", "P5", "P4", "P3", "P2", "P1", "P0"]
    );
}
