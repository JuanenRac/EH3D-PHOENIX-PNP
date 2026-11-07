//! The recipe of a job: the file and its checks, the limits, the codes in the eight languages, the steps and the parameters by package, feeder and tip that the job takes from it, the command
//! `recipe-check`, `simulate --recipe`, and the feeder and the pick that a saved state remembers for the decisions of the person.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::error::{Code, Lang};
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::job::{Action, Job, Phase};
use eh3d_phoenix_pnp::model::Machine;
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use eh3d_phoenix_pnp::recipe::{Limits, Params, Recipe, Selector, Step, RECIPE_VERSION};
use eh3d_phoenix_pnp::sim::SimOp;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const EXAMPLE: &str = include_str!("../examples/job.recipe");
const BASIC: &str = include_str!("../examples/job_basic.recipe");
const MACHINE: &str = include_str!("../examples/machine.cfg");
const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nR3,10k,R,30,10,180,top\nC1,100n,C,10,20,0,top\nC2,100n,C,20,20,0,top\nU1,MCU,Q,40,15,0,top\n";
const FEEDERS: &str =
    "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\n";
const SIX_RESISTORS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,0,top\nR3,10k,R,30,10,0,top\nR4,10k,R,40,10,0,top\nR5,10k,R,50,10,0,top\nR6,10k,R,60,10,0,top\n";
const TWO_FEEDERS: &str =
    "id,value,package,x,y,count,priority\nFA,10k,R,-20,10,10,high\nFB,10k,R,-20,30,10,normal\n";
const CHECK_STEPS: &str = "step = prepare
step = pick
step = place
step = check
";
const ALL_STEPS: &str = "step = prepare\nstep = pick\nstep = align\nstep = place\nstep = check\n";

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn limits() -> Limits {
    Limits::default()
}

fn parse(text: &str) -> Result<Recipe, eh3d_phoenix_pnp::error::PnpError> {
    Recipe::parse(text, &limits())
}

fn with_version(rest: &str) -> String {
    format!("recipe_version = 1\n{rest}")
}

/// The job of `parts` and `feeders` with the configuration `cfg`, made the way `simulate --recipe` makes it when a recipe is given.
fn job_of(parts: &str, feeders: &str, cfg: &str, recipe: Option<&str>) -> Job {
    let mut cfg = MachineConfig::parse(cfg).unwrap();
    let recipe = recipe.map(|t| parse(t).unwrap());
    if let Some(r) = &recipe {
        cfg = r.apply(&cfg);
    }
    let parts = from_csv(parts).unwrap();
    let feeders = feeding::from_csv(feeders).unwrap();
    let picks = assign(&parts, &feeders, Some(Side::Top));
    let plan = plan_advanced(
        &parts,
        &feeders,
        &picks,
        &Catalog::new(),
        &cfg,
        &Options::default(),
    )
    .unwrap();
    let mut job = Job::new(plan, feeders, cfg).unwrap();
    if let Some(r) = recipe {
        job.set_recipe(r).unwrap();
    }
    job
}

fn state(j: &Job) -> Value {
    serde_json::from_str(&j.save_state()).unwrap()
}

// ---- the file

#[test]
fn the_example_recipes_are_valid_and_say_what_they_hold() {
    let r = parse(EXAMPLE).unwrap();
    assert_eq!(r.name, "standard");
    assert_eq!(r.steps, Step::ALL.to_vec());
    assert_eq!(r.rules.len(), 4);
    assert_eq!(
        r.summary(),
        "ok: recipe 'standard', 5 steps (prepare, pick, align, place, check), 4 rules"
    );
    assert!(r.has(Step::Align) && r.has(Step::Check) && r.has_rules());
    assert_eq!(r.rules[0].selector, Selector::Package);
    assert_eq!(r.rules[0].pattern, "QFN-*");
    let b = parse(BASIC).unwrap();
    assert_eq!(b.steps, vec![Step::Prepare, Step::Pick, Step::Place]);
    assert_eq!(b.name, "basic");
    assert!(!b.has_rules() && b.rules.is_empty());
    assert_eq!(
        b.summary(),
        "ok: recipe 'basic', 3 steps (prepare, pick, place), 0 rules"
    );
    assert_eq!(RECIPE_VERSION, 1);
    assert_eq!(Recipe::standard().steps.len(), 5);
}

#[test]
fn the_values_of_the_rules_are_read_exactly() {
    let r = parse(EXAMPLE).unwrap();
    let p = r.rules[0].params;
    assert_eq!(p.pick_retries, Some(3));
    assert!(near(p.pick_tolerance_mm.unwrap(), 0.3));
    assert_eq!((p.vacuum_suck_s, p.vacuum_blow_s), (None, None));
    let tip = r.rules[3].params;
    assert!(near(tip.vacuum_suck_s.unwrap(), 0.12) && near(tip.vacuum_blow_s.unwrap(), 0.1));
    assert_eq!(tip.pick_retries, None);
    // the order of the keys, the case, the spaces and the comments do not matter, and a file made on Windows reads the same
    let text = "# a comment\r\n\r\n  RECIPE_VERSION =  1 \r\nName = My recipe\r\nSTEP = Prepare\r\nstep=pick\r\nstep = place\r\n  Package = r_* :  Vacuum_Suck_S = 0.2 , pick_retries=0\r\n";
    let r = parse(text).unwrap();
    assert_eq!(r.name, "My recipe");
    assert_eq!(r.steps, vec![Step::Prepare, Step::Pick, Step::Place]);
    assert_eq!(r.rules[0].params.pick_retries, Some(0));
    assert!(near(r.rules[0].params.vacuum_suck_s.unwrap(), 0.2));
    // a byte order mark at the start is ignored
    assert!(parse(&format!(
        "\u{feff}{}",
        with_version("step = prepare\nstep = pick\nstep = place\n")
    ))
    .is_ok());
}

#[test]
fn the_ends_of_the_ranges_are_valid_and_the_next_value_is_not() {
    let base = "step = prepare\nstep = pick\nstep = place\n";
    let with = |rule: &str| parse(&with_version(&format!("{base}{rule}\n")));
    for ok in [
        "package = A : pick_retries=0",
        "package = A : pick_retries=10",
        "package = A : pick_tolerance_mm=0.001",
        "package = A : pick_tolerance_mm=50",
        "package = A : vacuum_suck_s=0",
        "package = A : vacuum_suck_s=10",
        "package = A : vacuum_blow_s=0",
        "package = A : vacuum_blow_s=10",
    ] {
        assert!(with(ok).is_ok(), "{ok}");
    }
    for bad in [
        "package = A : pick_retries=11",
        "package = A : pick_retries=-1",
        "package = A : pick_retries=1.5",
        "package = A : pick_tolerance_mm=0.0009",
        "package = A : pick_tolerance_mm=50.1",
        "package = A : vacuum_suck_s=-0.1",
        "package = A : vacuum_suck_s=10.1",
        "package = A : vacuum_blow_s=10.1",
        "package = A : vacuum_blow_s=nan",
        "package = A : vacuum_blow_s=inf",
        "package = A : vacuum_blow_s=",
    ] {
        let e = with(bad).unwrap_err();
        assert_eq!(e.code, Code::RecipeBad, "{bad}");
        assert!(e.detail.starts_with("line 5: "), "{bad}: {}", e.detail);
    }
}

#[test]
fn a_recipe_that_is_not_valid_says_the_code_and_the_line() {
    let steps = "step = prepare\nstep = pick\nstep = place\n";
    let cases: Vec<(String, Code, &str)> = vec![
        (
            String::new(),
            Code::RecipeBad,
            "line 1: there is no recipe_version",
        ),
        (
            "# nothing\n".to_string(),
            Code::RecipeBad,
            "there is no recipe_version",
        ),
        (
            "name = x\n".to_string(),
            Code::RecipeBad,
            "line 1: recipe_version must be the first key",
        ),
        (
            "recipe_version = 0\n".to_string(),
            Code::RecipeBad,
            "line 1: recipe_version is not a whole number from 1",
        ),
        (
            "recipe_version = one\n".to_string(),
            Code::RecipeBad,
            "line 1: recipe_version is not a whole number from 1",
        ),
        (
            "recipe_version = 1\nrecipe_version = 1\n".to_string(),
            Code::RecipeBad,
            "line 2: recipe_version is written twice",
        ),
        (
            "recipe_version = 2\n".to_string(),
            Code::RecipeNewer,
            "line 1: recipe_version 2, this program reads up to 1",
        ),
        (
            "recipe_version\n".to_string(),
            Code::RecipeBad,
            "line 1: it is not key = value",
        ),
        (
            with_version("color = red\n"),
            Code::RecipeBad,
            "line 2: color is not a key of a recipe",
        ),
        (
            with_version("name = a\nname = b\n"),
            Code::RecipeBad,
            "line 3: name is written twice",
        ),
        (
            with_version("name =\n"),
            Code::RecipeBad,
            "line 2: the name is empty",
        ),
        (
            with_version("step = jump\n"),
            Code::RecipeBad,
            "line 2: 'jump' is not a step",
        ),
        (
            with_version(&format!("{steps}package = A\n")),
            Code::RecipeBad,
            "line 5: package needs PATTERN : key=value, ...",
        ),
        (
            with_version(&format!("{steps}package = : pick_retries=1\n")),
            Code::RecipeBad,
            "line 5: the pattern is empty",
        ),
        (
            with_version(&format!("{steps}tip = T :\n")),
            Code::RecipeBad,
            "line 5: '' is not key=value",
        ),
        (
            with_version(&format!("{steps}tip = T : pick_retries\n")),
            Code::RecipeBad,
            "line 5: 'pick_retries' is not key=value",
        ),
        (
            with_version(&format!("{steps}feeder = F : colour=1\n")),
            Code::RecipeBad,
            "line 5: colour is not a parameter of a rule",
        ),
        (
            with_version(&format!(
                "{steps}feeder = F : pick_retries=1, pick_retries=2\n"
            )),
            Code::RecipeBad,
            "line 5: pick_retries is written twice in the rule",
        ),
        (
            with_version(&format!(
                "{steps}package = A : pick_retries=1\npackage = a : pick_retries=2\n"
            )),
            Code::RecipeBad,
            "line 6: package 'a' has a rule already",
        ),
        (
            with_version("step = pick\nstep = prepare\nstep = place\n"),
            Code::RecipeSteps,
            "'prepare' cannot come after 'pick'",
        ),
        (
            with_version("step = prepare\nstep = pick\nstep = pick\nstep = place\n"),
            Code::RecipeSteps,
            "'pick' cannot come after 'pick'",
        ),
        (
            with_version("step = prepare\nstep = pick\nstep = place\nstep = align\n"),
            Code::RecipeSteps,
            "'align' cannot come after 'place'",
        ),
        (
            with_version("step = prepare\nstep = pick\n"),
            Code::RecipeSteps,
            "the step 'place' is missing (steps: prepare, pick)",
        ),
        (
            with_version("step = pick\nstep = place\n"),
            Code::RecipeSteps,
            "the step 'prepare' is missing",
        ),
        (
            with_version(""),
            Code::RecipeSteps,
            "the step 'prepare' is missing (steps: none)",
        ),
    ];
    for (text, code, fragment) in cases {
        let e = parse(&text).unwrap_err();
        assert_eq!(e.code, code, "{text:?}: {}", e.detail);
        assert!(e.detail.contains(fragment), "{text:?}: {}", e.detail);
        for lang in Lang::ALL {
            assert!(e.localized(lang).contains(&code.label()), "{lang:?}");
        }
    }
}

#[test]
fn the_limits_are_the_ones_of_the_configuration() {
    let d = MachineConfig::default();
    assert_eq!((d.recipe_max_bytes, d.recipe_max_rules), (65_536, 64));
    assert_eq!(Limits::from_config(&d), limits());
    // the bytes: a file with exactly the limit is read, one byte more is not
    let ok = with_version("step = prepare\nstep = pick\nstep = place\n");
    let exact = Limits {
        max_bytes: ok.len() as u64,
        max_rules: 64,
    };
    assert!(Recipe::parse(&ok, &exact).is_ok());
    let e = Recipe::parse(
        &ok,
        &Limits {
            max_bytes: ok.len() as u64 - 1,
            max_rules: 64,
        },
    )
    .unwrap_err();
    assert_eq!(e.code, Code::RecipeLimit);
    assert_eq!(
        e.detail,
        format!(
            "the recipe has {} bytes, the limit is {}",
            ok.len(),
            ok.len() - 1
        )
    );
    // the rules: the example has four, so three is too few and the fourth rule is the one that says so
    let four = Limits {
        max_bytes: 65_536,
        max_rules: 4,
    };
    assert!(Recipe::parse(EXAMPLE, &four).is_ok());
    let three = Limits {
        max_bytes: 65_536,
        max_rules: 3,
    };
    let e = Recipe::parse(EXAMPLE, &three).unwrap_err();
    assert_eq!(e.code, Code::RecipeLimit);
    let at = EXAMPLE
        .lines()
        .position(|l| l.starts_with("tip = N75"))
        .unwrap()
        + 1;
    assert_eq!(e.detail, format!("line {at}: more than 3 rules"));
    // the line: 1024 characters are read, 1025 are not
    let name = |n: usize| {
        with_version(&format!(
            "name = {}\nstep = prepare\nstep = pick\nstep = place\n",
            "a".repeat(n)
        ))
    };
    let e = parse(&name(1100)).unwrap_err();
    assert_eq!(
        (e.code, e.detail.as_str()),
        (Code::RecipeLimit, "line 2 has more than 1024 characters")
    );
    // a name of 64 characters is valid and one of 65 is not
    assert_eq!(parse(&name(64)).unwrap().name.len(), 64);
    assert_eq!(parse(&name(65)).unwrap_err().code, Code::RecipeBad);
    let long_line = format!("recipe_version = 1\n# {}\n", "x".repeat(1022));
    assert_eq!(long_line.lines().nth(1).unwrap().chars().count(), 1024);
    assert_eq!(
        parse(&format!(
            "{long_line}step = prepare\nstep = pick\nstep = place\n"
        ))
        .map(|r| r.steps.len()),
        Ok(3)
    );
    let e = parse(&format!("recipe_version = 1\n# {}\n", "x".repeat(1023))).unwrap_err();
    assert_eq!(e.code, Code::RecipeLimit);
}

#[test]
fn the_settings_of_the_limits_have_their_ranges_and_the_example_is_the_default() {
    for key in ["recipe_max_bytes", "recipe_max_rules"] {
        assert!(KEYS.contains(&key), "{key}");
    }
    let e = MachineConfig::parse(MACHINE).unwrap();
    let d = MachineConfig::default();
    assert_eq!(e.recipe_max_bytes, d.recipe_max_bytes);
    assert_eq!(e.recipe_max_rules, d.recipe_max_rules);
    for (key, ok, bad) in [
        ("recipe_max_bytes", ["256", "1048576"], ["255", "1048577"]),
        ("recipe_max_rules", ["1", "1000"], ["0", "1001"]),
    ] {
        for v in ok {
            assert!(
                MachineConfig::parse(&format!("{key} = {v}")).is_ok(),
                "{key} {v}"
            );
        }
        for v in bad {
            let err = MachineConfig::parse(&format!("{key} = {v}")).unwrap_err();
            assert_eq!(err.code, Code::ConfigOutOfRange, "{key} {v}");
        }
    }
    let c = MachineConfig::parse("recipe_max_bytes = 4096\nrecipe_max_rules = 7").unwrap();
    assert_eq!(
        Limits::from_config(&c),
        Limits {
            max_bytes: 4096,
            max_rules: 7
        }
    );
}

#[test]
fn the_codes_have_their_meaning_in_the_eight_languages() {
    assert_eq!(Code::ALL.len(), 56);
    for (code, number) in [
        (Code::RecipeBad, 49),
        (Code::RecipeSteps, 50),
        (Code::RecipeLimit, 51),
        (Code::RecipeNewer, 52),
    ] {
        assert_eq!(code.number(), number);
        assert_eq!(code.label(), format!("PHOENIX-PNP-0{number}"));
        let mut seen = std::collections::HashSet::new();
        for lang in Lang::ALL {
            assert!(!code.message(lang).is_empty());
            seen.insert(code.message(lang));
        }
        assert_eq!(seen.len(), 8, "{}", code.label());
    }
    assert_eq!(
        Code::RecipeBad.message(Lang::Pt),
        "Uma linha da receita não é válida"
    );
    assert_eq!(
        Code::RecipeSteps.message(Lang::Pt),
        "Os passos da receita não estão na ordem do trabalho"
    );
    assert_eq!(
        Code::RecipeLimit.message(Lang::Pt),
        "A receita excede os limites permitidos"
    );
    assert_eq!(
        Code::RecipeNewer.message(Lang::Pt),
        "A receita foi escrita para uma versão mais recente do programa"
    );
    assert!(Code::RecipeBad.message(Lang::Es).contains("receta"));
}

// ---- the parameters

#[test]
fn the_first_rule_that_matches_decides_each_key_and_the_feeder_goes_first() {
    let r = parse(EXAMPLE).unwrap();
    let p = r.resolve("F1", "QFN-32", "N75");
    assert_eq!(p.pick_retries, Some(3));
    assert!(near(p.pick_tolerance_mm.unwrap(), 0.3));
    assert!(near(p.vacuum_suck_s.unwrap(), 0.12) && near(p.vacuum_blow_s.unwrap(), 0.1));
    // the feeder F4 wins over the package for the retries and leaves the other keys to the package and the tip
    let p = r.resolve("F4", "QFN-32", "N75");
    assert_eq!(p.pick_retries, Some(1));
    assert!(near(p.pick_tolerance_mm.unwrap(), 0.3) && near(p.vacuum_suck_s.unwrap(), 0.12));
    // the package R_* gives the suck time, and the tip N45 gives nothing
    let p = r.resolve("F4", "R_0603", "N45");
    assert_eq!(p.pick_retries, Some(1));
    assert!(near(p.vacuum_suck_s.unwrap(), 0.05));
    assert_eq!((p.pick_tolerance_mm, p.vacuum_blow_s), (None, None));
    // the package beats the tip for a key they both give
    let p = r.resolve("F9", "R_0402", "N75");
    assert!(near(p.vacuum_suck_s.unwrap(), 0.05) && near(p.vacuum_blow_s.unwrap(), 0.1));
    // the case and the spaces do not matter
    assert_eq!(
        r.resolve(" f4 ", "qfn-32", "n75 "),
        r.resolve("F4", "QFN-32", "N75")
    );
    assert!(r.resolve("F9", "C_0603", "N45").is_empty());
    assert_eq!(Params::default().or(Params::default()), Params::default());
    // inside a selector the rule that comes first in the file wins
    let r = parse(&with_version(
        "step = prepare\nstep = pick\nstep = place\npackage = R_* : pick_retries=5\npackage = R_06* : pick_retries=7, vacuum_suck_s=1\n",
    ))
    .unwrap();
    let p = r.resolve("F1", "R_0603", "");
    assert_eq!(p.pick_retries, Some(5));
    assert!(near(p.vacuum_suck_s.unwrap(), 1.0));
}

#[test]
fn the_steps_decide_the_align_step_and_the_vacuum_check_of_the_configuration() {
    let on = MachineConfig::parse("align = on\nvacuum = on\nsim_seed = 9").unwrap();
    let off = MachineConfig::parse("align = off\nvacuum = off\nsim_seed = 9").unwrap();
    let basic = parse(BASIC).unwrap();
    let applied = basic.apply(&on);
    assert!(!applied.align && !applied.vacuum);
    assert_eq!(applied.sim_seed, 9);
    let full = parse(EXAMPLE).unwrap().apply(&off);
    assert!(full.align && full.vacuum);
    let only_align = parse(&with_version(
        "step = prepare\nstep = pick\nstep = align\nstep = place\n",
    ))
    .unwrap();
    let c = only_align.apply(&on);
    assert!(c.align && !c.vacuum);
    // the configuration that was given is not changed
    assert!(on.align && on.vacuum && !off.align);
}

#[test]
fn a_recipe_with_no_rules_changes_nothing_in_the_job() {
    let cfg =
        "align = on\nvacuum = on\nsim_align_dx_mm = 0.1\nsim_seed = 3\nsim_vacuum_noise_kpa = 2";
    let mut plain = job_of(PARTS, FEEDERS, cfg, None);
    let mut with = job_of(PARTS, FEEDERS, cfg, Some(&with_version(ALL_STEPS)));
    assert_eq!(plain.hash(), with.hash());
    assert_eq!(plain.run(None).unwrap(), Phase::Finished);
    assert_eq!(with.run(None).unwrap(), Phase::Finished);
    assert_eq!(plain.save_state(), with.save_state());
    assert_eq!(plain.stats_json(), with.stats_json());
    assert!(plain.recipe().is_none() && with.recipe().is_some());
}

#[test]
fn a_recipe_without_the_align_step_and_the_check_does_the_job_without_them() {
    let cfg = "align = on\nvacuum = on\nsim_align_dx_mm = 0.1";
    let mut plain = job_of(PARTS, FEEDERS, cfg, None);
    plain.run(None).unwrap();
    assert_eq!(plain.stats().align.corrected, 6);
    assert_eq!(plain.stats().vacuum.pick_checks, 6);
    let mut basic = job_of(PARTS, FEEDERS, cfg, Some(BASIC));
    assert_eq!(basic.run(None).unwrap(), Phase::Finished);
    assert_eq!(basic.stats().placed, 6);
    assert_eq!(basic.stats().align.corrected, 0);
    assert_eq!(basic.stats().vacuum.pick_checks, 0);
    // and the machine is quicker for it
    assert!(basic.machine().now_us() < plain.machine().now_us());
    // the other way: the recipe turns on what the configuration had off
    let mut full = job_of(
        PARTS,
        FEEDERS,
        "sim_align_dx_mm = 0.1",
        Some(&with_version(ALL_STEPS)),
    );
    full.run(None).unwrap();
    assert_eq!(
        (
            full.stats().align.corrected,
            full.stats().vacuum.pick_checks
        ),
        (6, 6)
    );
}

#[test]
fn the_retries_of_a_pick_come_from_the_rule_that_matches() {
    let cfg = "error_policy = defer\nsim_pick_fail_rate = 1\npick_retries = 0\nfault_limit = 0";
    let attempts = |recipe: Option<&str>| {
        let mut j = job_of(PARTS, FEEDERS, cfg, recipe);
        assert_eq!(j.run(None).unwrap(), Phase::Finished);
        assert_eq!(j.stats().deferred, 6);
        j.stats().pick_attempts
    };
    let steps = "step = prepare\nstep = pick\nstep = place\n";
    // every part fails every attempt, so a part takes retries + 1 attempts: six parts with no retry are six attempts
    assert_eq!(attempts(None), 6);
    assert_eq!(attempts(Some(&with_version(steps))), 6);
    // three resistors with 3 retries are 4 attempts each, the other three parts one each: 12 + 3
    let by_package = with_version(&format!("{steps}package = R : pick_retries=3\n"));
    assert_eq!(attempts(Some(&by_package)), 15);
    // the feeder F1 holds the resistors: its rule wins over the one of the package, 2 attempts each: 6 + 3
    let by_feeder = with_version(&format!(
        "{steps}package = R : pick_retries=3\nfeeder = F1 : pick_retries=1\n"
    ));
    assert_eq!(attempts(Some(&by_feeder)), 9);
    // the tip of every part is the default one of the planner
    let by_tip = with_version(&format!("{steps}tip = * : pick_retries=2\n"));
    assert_eq!(attempts(Some(&by_tip)), 18);
    // a rule that matches nothing changes nothing
    let none = with_version(&format!("{steps}package = BGA-* : pick_retries=9\n"));
    assert_eq!(attempts(Some(&none)), 6);
}

#[test]
fn the_tolerance_of_the_camera_comes_from_the_rule_that_matches() {
    let cfg = "sim_align_dx_mm = 0.4\nerror_policy = defer\npick_retries = 0\nfault_limit = 0";
    // the tolerance of the machine is 0.5 mm: a part 0.4 mm off the nozzle is good
    let mut ok = job_of(PARTS, FEEDERS, cfg, Some(&with_version(ALL_STEPS)));
    ok.run(None).unwrap();
    assert_eq!(
        (ok.stats().align.corrected, ok.stats().align.rejected),
        (6, 0)
    );
    let tight = with_version(&format!("{ALL_STEPS}package = Q : pick_tolerance_mm=0.3\n"));
    let mut j = job_of(PARTS, FEEDERS, cfg, Some(&tight));
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    assert_eq!(
        (j.stats().align.corrected, j.stats().align.rejected),
        (5, 1)
    );
    assert_eq!((j.stats().placed, j.stats().deferred), (5, 1));
    assert!(j.deferred()[0]
        .1
        .contains("0.400 mm from the nozzle, the tolerance is 0.3 mm"));
    // a larger tolerance lets the part through
    let wide = with_version(&format!(
        "{ALL_STEPS}package = * : pick_tolerance_mm=0.45\n"
    ));
    let mut w = job_of(
        PARTS,
        FEEDERS,
        "sim_align_dx_mm = 0.4\npick_retries = 0\nfault_limit = 0\npick_tolerance_mm = 0.2",
        Some(&wide),
    );
    w.run(None).unwrap();
    assert_eq!(
        (w.stats().align.corrected, w.stats().align.rejected),
        (6, 0)
    );
}

#[test]
fn the_times_of_the_vacuum_come_from_the_rule_that_matches() {
    // 0.08 s of suction and 0.05 s of blowing by default; the rule asks for 0.5 and 0.25 for each of the six parts: 6 x (0.42 + 0.20) = 3.72 s
    let mut plain = job_of(PARTS, FEEDERS, "vacuum = on", None);
    plain.run(None).unwrap();
    let slow = with_version(&format!(
        "{CHECK_STEPS}package = * : vacuum_suck_s=0.5, vacuum_blow_s=0.25\n"
    ));
    let mut j = job_of(PARTS, FEEDERS, "vacuum = on", Some(&slow));
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    assert_eq!(j.machine().now_us() - plain.machine().now_us(), 3_720_000);
    // a rule of one feeder changes only its parts: F3 holds the one microcontroller, 0.42 s more for it
    let one = with_version(&format!("{CHECK_STEPS}feeder = F3 : vacuum_suck_s=0.5\n"));
    let mut k = job_of(PARTS, FEEDERS, "vacuum = on", Some(&one));
    k.run(None).unwrap();
    assert_eq!(k.machine().now_us() - plain.machine().now_us(), 420_000);
}

#[test]
fn a_recipe_is_taken_once_and_must_fit_the_configuration_of_the_job() {
    let mut j = job_of(PARTS, FEEDERS, "", None);
    let full = parse(&with_version(ALL_STEPS)).unwrap();
    // the job has no align step and no vacuum check, the recipe has both
    assert_eq!(j.set_recipe(full).unwrap_err().code, Code::RecipeSteps);
    assert!(j.recipe().is_none());
    j.set_recipe(parse(BASIC).unwrap()).unwrap();
    assert_eq!(j.recipe().unwrap().name, "basic");
    assert_eq!(
        j.set_recipe(parse(BASIC).unwrap()).unwrap_err().code,
        Code::JobActionBad
    );
}

#[test]
fn a_state_saved_with_other_rules_is_not_resumed_by_mistake() {
    let cfg = "pick_retries = 0\nsim_pick_fail_rate = 0.5\nerror_policy = defer\nfault_limit = 0";
    let rule_a =
        with_version("step = prepare\nstep = pick\nstep = place\npackage = R : pick_retries=1\n");
    let rule_b =
        with_version("step = prepare\nstep = pick\nstep = place\npackage = R : pick_retries=2\n");
    let mut a = job_of(PARTS, FEEDERS, cfg, Some(&rule_a));
    let b = job_of(PARTS, FEEDERS, cfg, Some(&rule_b));
    let plain = job_of(PARTS, FEEDERS, cfg, None);
    assert_ne!(a.hash(), b.hash());
    assert_ne!(a.hash(), plain.hash());
    a.run(Some(3)).unwrap();
    let saved = a.save_state();
    let mut again = job_of(PARTS, FEEDERS, cfg, Some(&rule_a));
    again.restore_state(&saved).unwrap();
    assert_eq!(again.save_state(), saved);
    for other in [b, plain] {
        let mut other = other;
        assert_eq!(
            other.restore_state(&saved).unwrap_err().code,
            Code::JobStateMismatch
        );
    }
}

// ---- the decisions of the person after a state is loaded

fn stopped_on_the_second_feeder() -> Job {
    let cfg = "pick_retries = 0\nerror_policy = defer\nfault_window = 3\nfault_limit = 2\nfault_action = alert\n";
    let mut j = job_of(SIX_RESISTORS, TWO_FEEDERS, cfg, None);
    // the picks 1 and 2 fail on FA, which stops the job; then FA is switched off and the picks 4 and 5 fail on FB
    for nth in [1, 2, 4, 5] {
        j.machine_mut().force_fail(SimOp::Pick, nth);
    }
    assert_eq!(j.run(None).unwrap_err().code, Code::FeederFaultWindow);
    assert_eq!(j.failure().unwrap().feeder, "FA");
    j.act(Action::SwitchOffFeeder).unwrap();
    assert_eq!(j.run(None).unwrap_err().code, Code::FeederFaultWindow);
    j
}

#[test]
fn the_feeder_that_stopped_the_job_is_kept_in_the_saved_state() {
    let j = stopped_on_the_second_feeder();
    let f = j.failure().unwrap();
    // the part is planned for FA, which the person switched off: the one that failed is FB
    assert_eq!(j.plan().plan.placements[f.index].feeder, "FA");
    assert_eq!(f.feeder, "FB");
    let doc = state(&j);
    assert_eq!(doc["last_feeder"], "FB");
    assert_eq!(doc["stopped_pick"], f.index);
    // another run loads the state: the dialog names FB and the person can switch it off
    let mut again = job_of(SIX_RESISTORS, TWO_FEEDERS, "pick_retries = 0\nerror_policy = defer\nfault_window = 3\nfault_limit = 2\nfault_action = alert\n", None);
    again.restore_state(&j.save_state()).unwrap();
    assert_eq!(again.phase(), Phase::Stopped);
    assert_eq!(again.failure().unwrap().feeder, "FB");
    again.act(Action::SwitchOffFeeder).unwrap();
    assert!(again.stats().feeders[0].disabled && again.stats().feeders[1].disabled);
    assert_eq!(
        state(&again)["trace"]["actions"],
        serde_json::json!([0, 0, 0, 2])
    );
}

#[test]
fn a_state_saved_before_they_were_kept_still_loads_and_uses_the_planned_feeder() {
    let j = stopped_on_the_second_feeder();
    let mut doc = state(&j);
    doc.as_object_mut().unwrap().remove("last_feeder");
    doc.as_object_mut().unwrap().remove("stopped_pick");
    let mut old = job_of(SIX_RESISTORS, TWO_FEEDERS, "pick_retries = 0\nerror_policy = defer\nfault_window = 3\nfault_limit = 2\nfault_action = alert\n", None);
    old.restore_state(&doc.to_string()).unwrap();
    assert_eq!(old.failure().unwrap().feeder, "FA");
    // a feeder that is not in the list, or a pick outside the plan, is ignored as well
    doc["last_feeder"] = serde_json::json!("nowhere");
    doc["stopped_pick"] = serde_json::json!(999);
    let mut odd = job_of(SIX_RESISTORS, TWO_FEEDERS, "pick_retries = 0\nerror_policy = defer\nfault_window = 3\nfault_limit = 2\nfault_action = alert\n", None);
    odd.restore_state(&doc.to_string()).unwrap();
    assert_eq!(odd.failure().unwrap().feeder, "FA");
    assert!(state(&odd).get("last_feeder").is_none() && state(&odd).get("stopped_pick").is_none());
}

#[test]
fn the_pick_that_was_stopped_is_not_counted_as_one_at_the_first_attempt_after_the_state_is_loaded()
{
    let cfg = "pick_retries = 0\nerror_policy = alert\nsim_pick_fail_rate = 0\nfault_limit = 0\n";
    let mut j = job_of(PARTS, FEEDERS, cfg, None);
    j.machine_mut().force_fail(SimOp::Pick, 1);
    assert_eq!(j.run(None).unwrap_err().code, Code::PickFailed);
    let saved = j.save_state();
    let mut kept = job_of(PARTS, FEEDERS, cfg, None);
    kept.restore_state(&saved).unwrap();
    kept.act(Action::Retry).unwrap();
    kept.run(None).unwrap();
    // the first part was picked at the second try, in the run that went on from the state as in the one that never stopped
    j.act(Action::Retry).unwrap();
    j.run(None).unwrap();
    assert_eq!(kept.trace().first_try, 5);
    assert_eq!(kept.trace().retried, 1);
    assert_eq!((j.trace().first_try, j.trace().retried), (5, 1));
}

// ---- the command line

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn example(name: &str) -> String {
    root()
        .join("examples")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eh3d_pnp_recipe_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .output()
        .unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn path(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

#[test]
fn recipe_check_reads_the_examples_and_names_the_code_and_the_line_of_an_error() {
    let ok = run(&["recipe-check", &example("job.recipe")]);
    assert_eq!(ok.status.code(), Some(0), "{}", err(&ok));
    assert_eq!(
        out(&ok).trim(),
        "ok: recipe 'standard', 5 steps (prepare, pick, align, place, check), 4 rules"
    );
    let dir = scratch("check");
    let bad = path(&dir, "bad.recipe");
    std::fs::write(
        &bad,
        "recipe_version = 1\nstep = pick\nstep = prepare\nstep = place\n",
    )
    .unwrap();
    let e = run(&["recipe-check", &bad]);
    assert_eq!(e.status.code(), Some(1));
    assert!(err(&e).contains("PHOENIX-PNP-050"), "{}", err(&e));
    let es = run(&["recipe-check", &bad, "--lang", "es"]);
    assert!(
        err(&es).contains("PHOENIX-PNP-050") && err(&es).contains("receta"),
        "{}",
        err(&es)
    );
    let pt = run(&["recipe-check", &bad, "--lang", "pt"]);
    assert!(err(&pt).contains("Os passos da receita"), "{}", err(&pt));
    // the limits come from --config
    let cfg = path(&dir, "small.cfg");
    std::fs::write(&cfg, "recipe_max_bytes = 256\n").unwrap();
    let big = run(&["recipe-check", &example("job.recipe"), "--config", &cfg]);
    assert_eq!(big.status.code(), Some(1));
    assert!(err(&big).contains("PHOENIX-PNP-051"), "{}", err(&big));
    assert_eq!(run(&["recipe-check"]).status.code(), Some(1));
    assert_eq!(
        run(&["recipe-check", &bad, "--lang", "xx"]).status.code(),
        Some(1)
    );
    let newer = path(&dir, "newer.recipe");
    std::fs::write(&newer, "recipe_version = 9\n").unwrap();
    assert!(err(&run(&["recipe-check", &newer])).contains("PHOENIX-PNP-052"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn simulate_follows_the_recipe_and_refuses_to_write_over_it() {
    let dir = scratch("simulate");
    let parts = path(&dir, "parts.csv");
    let feeders = path(&dir, "feeders.csv");
    std::fs::write(&parts, PARTS).unwrap();
    std::fs::write(&feeders, FEEDERS).unwrap();
    let plain = run(&["simulate", &parts, &feeders]);
    assert_eq!(plain.status.code(), Some(0), "{}", err(&plain));
    assert!(!out(&plain).contains("align:") && !out(&plain).contains("vacuum:"));
    let recipe = path(&dir, "full.recipe");
    std::fs::write(&recipe, with_version(ALL_STEPS)).unwrap();
    let full = run(&["simulate", &parts, &feeders, "--recipe", &recipe]);
    assert_eq!(full.status.code(), Some(0), "{}", err(&full));
    let text = out(&full);
    assert!(text.contains("align: 6 parts corrected"), "{text}");
    assert!(
        text.contains("vacuum: 0 of 6 picks found no part"),
        "{text}"
    );
    assert!(text.contains("placed 6 of 6"), "{text}");
    // the example recipe works with the example tables
    let ex = run(&[
        "simulate",
        &example("board.pos"),
        &example("feeders.csv"),
        "--config",
        &example("machine.cfg"),
        "--packages",
        &example("packages.csv"),
        "--recipe",
        &example("job.recipe"),
    ]);
    assert_eq!(ex.status.code(), Some(0), "{}", err(&ex));
    assert!(out(&ex).contains("vacuum:"), "{}", out(&ex));
    // a recipe that is not valid stops the command before anything runs, and the report is never written over the recipe
    let bad = path(&dir, "bad.recipe");
    std::fs::write(&bad, "recipe_version = 1\nstep = place\n").unwrap();
    let e = run(&["simulate", &parts, &feeders, "--recipe", &bad]);
    assert_eq!(e.status.code(), Some(1));
    assert!(err(&e).contains("PHOENIX-PNP-050"), "{}", err(&e));
    let same = run(&[
        "simulate", &parts, &feeders, "--recipe", &recipe, "--report", &recipe,
    ]);
    assert_eq!(same.status.code(), Some(1));
    assert!(err(&same).contains("also an input"), "{}", err(&same));
    let _ = std::fs::remove_dir_all(&dir);
}
