//! Look, move, look again: it converges on a good camera, no step is ever longer than the cap, a look that gets worse can only make the steps smaller, a camera that cannot be read stops it
//! with no move, and the settings that drive it are in the file of the settings.

use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::lookmove::{align, report, Outcome, Params, Sim};

fn params() -> Params {
    Params::from_config(&MachineConfig::default())
}

fn sim(error: (f64, f64)) -> Sim {
    Sim {
        error,
        scale: 1.0,
        noise_mm: 0.0,
        seed: 1,
        blind_looks: 0,
    }
}

fn len(v: (f64, f64)) -> f64 {
    (v.0 * v.0 + v.1 * v.1).sqrt()
}

#[test]
fn the_defaults_come_from_the_settings_and_the_example_gives_the_same() {
    let d = params();
    let ex = MachineConfig::parse(include_str!("../examples/machine.cfg")).unwrap();
    assert_eq!(Params::from_config(&ex), d);
    assert_eq!((d.max_step_mm, d.max_iterations, d.gain), (0.5, 4, 0.8));
}

#[test]
fn a_good_camera_brings_the_head_to_the_mark_in_a_few_looks() {
    let r = sim((1.2, -0.8)).run(&Params {
        max_iterations: 8,
        ..params()
    });
    assert_eq!(r.outcome, Outcome::Aligned, "{r:?}");
    assert!(
        (r.total.0 - 1.2).abs() < 0.05 && (r.total.1 + 0.8).abs() < 0.05,
        "{:?}",
        r.total
    );
    assert!(r.steps.len() <= 9);
    let last = r.steps.last().unwrap();
    assert!(last.residual_mm <= params().tolerance_mm && last.moved == (0.0, 0.0));
}

#[test]
fn a_mark_already_under_the_tolerance_is_aligned_at_the_first_look_with_no_move() {
    let r = sim((0.005, 0.0)).run(&params());
    assert_eq!(r.outcome, Outcome::Aligned);
    assert_eq!(r.steps.len(), 1);
    assert_eq!(r.total, (0.0, 0.0));
}

#[test]
fn no_step_is_longer_than_the_cap_whatever_the_error() {
    let p = Params {
        max_step_mm: 0.3,
        max_iterations: 6,
        ..params()
    };
    for error in [(50.0, 0.0), (-30.0, 40.0), (0.0, -1000.0), (0.31, 0.0)] {
        let r = sim(error).run(&p);
        for s in &r.steps {
            assert!(len(s.moved) <= s.cap_mm + 1e-9, "{error:?}: {s:?}");
            assert!(len(s.moved) <= p.max_step_mm + 1e-9);
        }
        assert!(len(r.total) <= p.max_step_mm * f64::from(p.max_iterations) + 1e-9);
    }
    let r = sim((50.0, 0.0)).run(&p);
    assert_eq!(r.outcome, Outcome::NotConverged);
    assert!(r.steps.iter().take(6).all(|s| s.clipped));
}

#[test]
fn the_cap_and_the_gain_only_ever_go_down() {
    // a camera that sees 6 times what is there makes the first moves overshoot: the looks get worse, the cap and the gain are halved, and they stay down
    let r = Sim {
        scale: 6.0,
        ..sim((2.0, 0.0))
    }
    .run(&Params {
        max_step_mm: 5.0,
        max_iterations: 12,
        ..params()
    });
    assert!(r.steps.len() >= 3, "{r:?}");
    for w in r.steps.windows(2) {
        assert!(w[1].cap_mm <= w[0].cap_mm, "{w:?}");
        assert!(w[1].gain <= w[0].gain, "{w:?}");
    }
    assert!(r.steps.last().unwrap().cap_mm < 5.0, "{r:?}");
    assert!(r.steps.iter().all(|s| len(s.moved) <= s.cap_mm + 1e-9));
}

#[test]
fn two_worse_looks_in_a_row_stop_the_run() {
    // every look is further off than the one before
    let mut n = 0.0;
    let r = align(
        &Params {
            max_iterations: 10,
            ..params()
        },
        |_, _| {
            n += 0.5;
            Some((n, 0.0))
        },
    );
    assert_eq!(r.outcome, Outcome::Diverging, "{r:?}");
    assert_eq!(r.steps.len(), 3);
    assert_eq!(
        r.steps.last().unwrap().moved,
        (0.0, 0.0),
        "it stops without another move"
    );
}

#[test]
fn a_camera_that_cannot_be_read_stops_the_run_with_no_move() {
    let r = Sim {
        blind_looks: 1,
        ..sim((1.0, 1.0))
    }
    .run(&params());
    assert_eq!(r.outcome, Outcome::Unreadable);
    assert!(r.steps.is_empty());
    assert_eq!(r.total, (0.0, 0.0));
    // a look that fails after some moves stops there and keeps what was done
    let mut n = 0;
    let r = align(&params(), |_, _| {
        n += 1;
        if n < 3 {
            Some((1.0, 0.0))
        } else {
            None
        }
    });
    assert_eq!(r.outcome, Outcome::Unreadable);
    assert_eq!(r.steps.len(), 2);
    let r = align(&params(), |_, _| Some((f64::NAN, 0.0)));
    assert_eq!(r.outcome, Outcome::Unreadable);
}

#[test]
fn the_offset_of_the_camera_is_taken_off_what_is_seen() {
    let p = Params {
        offset: (0.3, -0.2),
        max_iterations: 8,
        ..params()
    };
    // the camera sees the head 0.3 mm off in X and -0.2 mm off in Y at the mark: that is its own constant error, so the head is aligned there
    let r = align(&p, |_, _| Some((0.3, -0.2)));
    assert_eq!(r.outcome, Outcome::Aligned);
    assert_eq!(r.total, (0.0, 0.0));
}

#[test]
fn noise_made_by_chance_on_a_seed_repeats_and_still_converges_when_it_is_small() {
    let s = Sim {
        noise_mm: 0.005,
        seed: 42,
        ..sim((0.9, 0.4))
    };
    let p = Params {
        max_iterations: 8,
        ..params()
    };
    let (a, b) = (s.run(&p), s.run(&p));
    assert_eq!(a, b);
    assert_eq!(a.outcome, Outcome::Aligned);
    for seed in 0..40u64 {
        let r = Sim { seed, ..s.clone() }.run(&p);
        assert!(
            matches!(r.outcome, Outcome::Aligned | Outcome::NotConverged),
            "{seed}: {r:?}"
        );
        assert!(r.steps.iter().all(|s| len(s.moved) <= p.max_step_mm + 1e-9));
    }
}

#[test]
fn the_report_is_in_the_eight_languages_and_names_the_outcome() {
    let r = sim((0.4, 0.0)).run(&params());
    let mut seen = std::collections::BTreeSet::new();
    for l in Lang::ALL {
        let t = report(&r, l);
        assert!(t.contains("aligned") && !t.contains('{'), "{t}");
        seen.insert(t);
    }
    assert_eq!(seen.len(), 8);
}

#[test]
fn the_keys_have_their_ranges() {
    for (line, bad) in [
        ("lml_max_step_mm = 0\n", true),
        ("lml_max_step_mm = 21\n", true),
        ("lml_tolerance_mm = 0\n", true),
        ("lml_max_iterations = 0\n", true),
        ("lml_max_iterations = 21\n", true),
        ("lml_gain = 0\n", true),
        ("lml_gain = 1.5\n", true),
        ("lml_offset_x_mm = 51\n", true),
        ("lml_gain = 1\n", false),
        ("lml_offset_y_mm = -50\n", false),
    ] {
        assert_eq!(MachineConfig::parse(line).is_err(), bad, "{line}");
    }
}
