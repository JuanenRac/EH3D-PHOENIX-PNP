//! The nozzle tips that a package accepts: the rules of the configuration, the choice of one tip for every kind of part with the fewest changes of tip, the tips of the catalog that are kept,
//! and the plan that follows.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, AdvancedPlan, Options};
use eh3d_phoenix_pnp::tips::{choose, resolve, rule_for, Need, TipChoice, TipRule};

const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nR3,10k,R,30,10,180,top\nC1,100n,C,10,20,0,top\nC2,100n,C,20,20,0,top\nU1,MCU,Q,40,15,0,top\n";
const FEEDERS: &str =
    "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\n";
const RULES: &str = "tip_compat = R : t-fine, t-wide\ntip_compat = C : t-wide, t-small\n";

fn need(candidates: &[&str], count: usize) -> Need {
    Need {
        candidates: candidates.iter().map(|t| t.to_string()).collect(),
        count,
    }
}

fn names(v: Vec<String>) -> Vec<String> {
    v
}

// ---- the choice

#[test]
fn a_tip_that_only_one_kind_accepts_is_taken_and_covers_the_kinds_that_also_accept_it() {
    // c is forced by the third kind; the second kind accepts it, so only the first kind is left, and a (preferred) covers as many parts as b
    let needs = [need(&["a", "b"], 4), need(&["b", "c"], 3), need(&["c"], 1)];
    assert_eq!(
        names(choose(&needs, TipChoice::FewestChanges)),
        ["a", "c", "c"]
    );
    // each kind takes its first tip: three tips instead of two
    assert_eq!(names(choose(&needs, TipChoice::Preferred)), ["a", "b", "c"]);
}

#[test]
fn the_tip_that_covers_the_most_parts_wins_and_the_preferred_one_breaks_a_tie() {
    // b covers 2 + 2 + 1 parts at a cost of 2 * 1 + 0 + 1 * 1 = 3; a covers 2, c covers 3: one tip serves every kind
    let needs = [
        need(&["a", "b"], 2),
        need(&["b", "c"], 2),
        need(&["c", "b"], 1),
    ];
    assert_eq!(
        names(choose(&needs, TipChoice::FewestChanges)),
        ["b", "b", "b"]
    );
    // x and y cover the same parts at the same cost: the name decides, and then both kinds use it
    let tie = [need(&["x", "y"], 1), need(&["y", "x"], 1)];
    assert_eq!(names(choose(&tie, TipChoice::FewestChanges)), ["x", "x"]);
    // equal coverage, different cost: the tip that is first in the lists of the larger kind
    let cost = [need(&["p", "q"], 3), need(&["q", "p"], 1)];
    assert_eq!(names(choose(&cost, TipChoice::FewestChanges)), ["p", "p"]);
}

#[test]
fn nothing_to_choose_gives_nothing_and_a_kind_without_tips_takes_the_empty_one() {
    assert!(choose(&[], TipChoice::FewestChanges).is_empty());
    assert!(choose(&[], TipChoice::Preferred).is_empty());
    let empty = Need {
        candidates: Vec::new(),
        count: 2,
    };
    assert_eq!(
        names(choose(
            &[empty.clone(), need(&["a"], 1)],
            TipChoice::FewestChanges
        )),
        ["", "a"]
    );
    assert_eq!(names(choose(&[empty], TipChoice::Preferred)), [""]);
}

#[test]
fn the_choice_does_not_depend_on_how_many_times_it_is_asked() {
    let needs = [
        need(&["a", "b", "c"], 5),
        need(&["c", "d"], 2),
        need(&["d", "a"], 2),
    ];
    let first = choose(&needs, TipChoice::FewestChanges);
    for _ in 0..20 {
        assert_eq!(choose(&needs, TipChoice::FewestChanges), first);
    }
    // a covers 5 + 2 = 7 parts at a cost of 2, c covers 7 at a cost of 10, no other tip covers more than 5; the second kind is left with c, the third takes a
    assert_eq!(names(first), ["a", "c", "a"]);
}

// ---- the rules

#[test]
fn the_first_rule_that_matches_the_package_is_the_one() {
    let rules = vec![
        TipRule {
            pattern: "QFN-*".to_string(),
            tips: vec!["big".to_string()],
        },
        TipRule {
            pattern: "*".to_string(),
            tips: vec!["any".to_string()],
        },
    ];
    assert_eq!(rule_for(&rules, "qfn-32").unwrap().tips, ["big"]);
    assert_eq!(rule_for(&rules, " R_0603 ").unwrap().tips, ["any"]);
    assert!(rule_for(&rules[..1], "R_0603").is_none());
    assert!(rule_for(&[], "R").is_none());
}

#[test]
fn the_rules_of_the_configuration_are_checked() {
    let c = MachineConfig::parse(&format!(
        "{RULES}tip_compat = QFN-* : big , small\ntip_compat_mode = preferred"
    ))
    .unwrap();
    assert_eq!(c.tip_rules.len(), 3);
    assert_eq!(c.tip_rules[2].pattern, "QFN-*");
    assert_eq!(c.tip_rules[2].tips, ["big", "small"]);
    assert_eq!(c.tip_choice, TipChoice::Preferred);
    assert_eq!(TipChoice::Preferred.name(), "preferred");
    assert_eq!(
        TipChoice::parse("fewest_changes"),
        Some(TipChoice::FewestChanges)
    );
    assert_eq!(TipChoice::parse("whatever"), None);
    for bad in [
        "tip_compat = R",
        "tip_compat = : a",
        "tip_compat = R :",
        "tip_compat = R : a, , b",
        "tip_compat = R : a, a",
        "tip_compat = R : a, b, c, d, e, f, g, h, i",
        "tip_compat_mode = best",
    ] {
        assert_eq!(
            MachineConfig::parse(bad).unwrap_err().code,
            Code::ConfigBadValue,
            "{bad}"
        );
    }
    // a pattern is written once
    assert_eq!(
        MachineConfig::parse("tip_compat = R : a\ntip_compat = R : b")
            .unwrap_err()
            .code,
        Code::ConfigBadValue
    );
    let many: String = (0..65)
        .map(|i| format!("tip_compat = P{i} : a\n"))
        .collect();
    assert_eq!(
        MachineConfig::parse(&many).unwrap_err().code,
        Code::ConfigBadValue
    );
    assert!(MachineConfig::parse(&many[..many.len() - "tip_compat = P64 : a\n".len()]).is_ok());
}

// ---- the plan

fn plan_of(cfg: &str, packages: &str) -> AdvancedPlan {
    let cfg = MachineConfig::parse(cfg).unwrap();
    let parts = from_csv(PARTS).unwrap();
    let feeders = feeding::from_csv(FEEDERS).unwrap();
    let picks = assign(&parts, &feeders, Some(Side::Top));
    let mut catalog = Catalog::new();
    if !packages.is_empty() {
        catalog.add_packages(packages).unwrap();
    }
    plan_advanced(
        &parts,
        &feeders,
        &picks,
        &catalog,
        &cfg,
        &Options::default(),
    )
    .unwrap()
}

fn tip_of(plan: &AdvancedPlan, reference: &str) -> String {
    let i = plan
        .plan
        .placements
        .iter()
        .position(|p| p.reference == reference)
        .unwrap();
    plan.details[i].tip.clone()
}

#[test]
fn without_rules_the_tips_are_what_they_always_were() {
    let plain = plan_of("", "");
    assert_eq!(plain.tip_changes, 0);
    let cfg = MachineConfig::parse("").unwrap();
    let parts = from_csv(PARTS).unwrap();
    let feeders = feeding::from_csv(FEEDERS).unwrap();
    let picks = assign(&parts, &feeders, Some(Side::Top));
    assert!(resolve(&parts, &picks, Side::Top, &Catalog::new(), &cfg).is_empty());
    // a rule that matches no package of the board changes nothing either
    assert_eq!(plan_of("tip_compat = SOT-23* : x, y", ""), plain);
}

#[test]
fn one_tip_serves_the_resistors_and_the_capacitors_and_the_plan_changes_it_once() {
    // R accepts t-fine and t-wide, C accepts t-wide and t-small: t-wide serves both (5 parts); the microcontroller has no rule and keeps the default tip
    let plan = plan_of(RULES, "");
    for r in ["R1", "R2", "R3", "C1", "C2"] {
        assert_eq!(tip_of(&plan, r), "t-wide", "{r}");
    }
    assert_eq!(tip_of(&plan, "U1"), "");
    assert_eq!(plan.tip_changes, 1);
    // each kind with its preferred tip: three tips, two changes
    let preferred = plan_of(&format!("{RULES}tip_compat_mode = preferred"), "");
    assert_eq!(tip_of(&preferred, "R1"), "t-fine");
    assert_eq!(tip_of(&preferred, "C1"), "t-wide");
    assert_eq!(tip_of(&preferred, "U1"), "");
    assert_eq!(preferred.tip_changes, 2);
}

#[test]
fn a_tip_named_by_the_catalog_is_kept_and_the_others_choose_around_it() {
    let plan = plan_of(RULES, "package,nozzle_tip\nR,t-fine\n");
    for r in ["R1", "R2", "R3"] {
        assert_eq!(tip_of(&plan, r), "t-fine", "{r}");
    }
    // the capacitors accept t-wide first and t-small second, and neither is in use: the preferred one
    assert_eq!(tip_of(&plan, "C1"), "t-wide");
    assert_eq!(tip_of(&plan, "U1"), "");
    assert_eq!(plan.tip_changes, 2);
}

#[test]
fn the_same_plan_comes_out_whatever_the_order_of_the_files() {
    let a = plan_of(RULES, "");
    let reversed: String = {
        let mut lines: Vec<&str> = PARTS.lines().collect();
        let header = lines.remove(0);
        lines.reverse();
        format!("{header}\n{}\n", lines.join("\n"))
    };
    let cfg = MachineConfig::parse(RULES).unwrap();
    let parts = from_csv(&reversed).unwrap();
    let feeders = feeding::from_csv(FEEDERS).unwrap();
    let picks = assign(&parts, &feeders, Some(Side::Top));
    let b = plan_advanced(
        &parts,
        &feeders,
        &picks,
        &Catalog::new(),
        &cfg,
        &Options::default(),
    )
    .unwrap();
    assert_eq!(a.plan.placements, b.plan.placements);
    assert_eq!(a.details, b.details);
}
