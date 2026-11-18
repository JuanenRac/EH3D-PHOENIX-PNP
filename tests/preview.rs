//! The picture of the plan: what it holds, that nothing in it can break the file, how it keeps to its size and the option `--svg` of `schedule`.

use eh3d_phoenix_pnp::config::OrderStrategy;
use eh3d_phoenix_pnp::feeders::Feeder;
use eh3d_phoenix_pnp::feeding::FeederSpec;
use eh3d_phoenix_pnp::plan::{Placement, Plan};
use eh3d_phoenix_pnp::planner::{AdvancedPlan, Detail};
use eh3d_phoenix_pnp::preview::{svg, MAX_LABELS};
use std::path::PathBuf;
use std::process::{Command, Output};

fn placement(
    r: &str,
    feeder: &str,
    pick: (f64, f64),
    at: (f64, f64),
    nozzle: usize,
    batch: usize,
) -> Placement {
    Placement {
        reference: r.into(),
        feeder: feeder.into(),
        pick,
        x: at.0,
        y: at.1,
        rotation: 0.0,
        nozzle,
        batch,
    }
}

fn plan_of(placements: Vec<Placement>) -> AdvancedPlan {
    let details = placements
        .iter()
        .map(|_| Detail {
            rank: 0,
            tier: 0,
            tip: String::new(),
            bin: 0,
            height_mm: 1.0,
            pick_retries: 2,
        })
        .collect();
    let batches = placements.iter().map(|p| p.batch + 1).max().unwrap_or(0);
    AdvancedPlan {
        plan: Plan {
            placements,
            batches,
            travel_mm: 123.4,
            seconds: 56.7,
        },
        details,
        tip_changes: 1,
        tiers: 1,
        rotation_s: 0.0,
        align_s: 0.0,
        order: OrderStrategy::Nearest,
        pick_times: Vec::new(),
    }
}

fn feeder(id: &str, x: f64, y: f64) -> FeederSpec {
    FeederSpec::from_basic(Feeder {
        id: id.into(),
        value: "10k".into(),
        package: "R_0603".into(),
        x,
        y,
        count: 10,
    })
}

fn three() -> AdvancedPlan {
    plan_of(vec![
        placement("R1", "F1", (-20.0, 10.0), (10.0, 10.0), 0, 0),
        placement("R2", "F1", (-20.0, 10.0), (20.0, 40.0), 1, 0),
        placement("C1", "F2", (-20.0, 20.0), (30.0, 20.0), 0, 1),
    ])
}

fn between<'a>(text: &'a str, from: &str, to: &str) -> &'a str {
    let a = text.find(from).unwrap() + from.len();
    let b = a + text[a..].find(to).unwrap();
    &text[a..b]
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
fn the_picture_holds_the_feeders_the_places_the_ways_and_the_totals() {
    let feeders = [feeder("F1", -20.0, 10.0), feeder("F2", -20.0, 20.0)];
    let s = svg(&three(), &feeders);
    assert!(
        s.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg "),
        "{s}"
    );
    assert!(s.ends_with("</svg>\n"), "{s}");
    assert_eq!(s.matches("<g ").count(), 4);
    assert_eq!(s.matches("</g>").count(), 4);
    assert!(
        s.contains("<title>R1: 1 from F1, nozzle 1, batch 1</title>"),
        "{s}"
    );
    assert!(
        s.contains("<title>R2: 2 from F1, nozzle 2, batch 1</title>"),
        "{s}"
    );
    assert!(
        s.contains("<title>C1: 3 from F2, nozzle 1, batch 2</title>"),
        "{s}"
    );
    assert!(s.contains("<title>F1 (10k / R_0603)</title>"), "{s}");
    assert!(
        s.contains("3 parts in 2 batches, travel 123 mm, about 57 s, 1 tip changes"),
        "{s}"
    );
    // a line from the feeder to the place for each part, in the colour of its nozzle
    assert_eq!(
        between(&s, "id=\"carry\"", "</g>")
            .matches("<line ")
            .count(),
        3
    );
    assert!(
        s.contains("stroke=\"#d9480f\"") && s.contains("stroke=\"#1971c2\""),
        "{s}"
    );
    // the way of the head: the picks of a batch, then its places, then the next batch
    let route = between(&s, "id=\"travel\"", "/>");
    let points = between(route, "points=\"", "\"");
    assert_eq!(points.split(' ').count(), 2 * 2 + 2);
    // the Y axis points up: the place with the larger Y is higher in the picture
    let cy = |title: &str| -> f64 {
        let at = s.find(title).unwrap();
        let tag = &s[..at];
        let c = tag.rfind("cy=\"").unwrap() + 4;
        tag[c..c + tag[c..].find('"').unwrap()].parse().unwrap()
    };
    assert!(cy("<title>R2:") < cy("<title>R1:"));
    // the legend names the nozzles that the plan uses
    assert!(
        s.contains(">nozzle 1</text>")
            && s.contains(">nozzle 2</text>")
            && !s.contains(">nozzle 3</text>")
    );
    // the same plan gives the same picture, and it holds nothing from outside
    assert_eq!(s, svg(&three(), &feeders));
    assert!(!s.contains("<script") && !s.contains("href") && s.matches("http").count() == 1);
}

#[test]
fn what_a_person_typed_cannot_break_the_picture() {
    let mut plan = three();
    plan.plan.placements[0].reference = "R<1>&\"x'\u{7}".to_string();
    plan.plan.placements[0].feeder = "F</title>".to_string();
    let mut f = feeder("<F1>", -20.0, 10.0);
    f.base.value = "a&b".to_string();
    let s = svg(&plan, &[f]);
    assert!(s.contains("R&lt;1&gt;&amp;&quot;x&apos; :"), "{s}");
    assert!(s.contains("F&lt;/title&gt;"), "{s}");
    assert!(
        s.contains("<title>&lt;F1&gt; (a&amp;b / R_0603)</title>"),
        "{s}"
    );
    assert!(!s.contains("R<1>") && !s.contains("F</title>") && !s.contains('\u{7}'));
    // every tag that is opened is closed
    assert_eq!(s.matches("<title>").count(), s.matches("</title>").count());
    assert_eq!(s.matches("<text ").count(), s.matches("</text>").count());
}

#[test]
fn an_empty_plan_a_place_that_is_not_a_number_and_a_feeder_that_is_off_are_drawn_without_trouble() {
    let empty = svg(&plan_of(Vec::new()), &[]);
    assert!(
        empty.contains("There is nothing in the plan to draw."),
        "{empty}"
    );
    assert!(empty.ends_with("</svg>\n"));
    let mut plan = three();
    plan.plan.placements[1].x = f64::NAN;
    let mut off = feeder("OFF", 0.0, 0.0);
    off.enabled = false;
    let s = svg(&plan, &[off, feeder("BAD", f64::INFINITY, 0.0)]);
    assert!(!s.contains("NaN") && !s.contains("inf"), "{s}");
    assert!(!s.contains("<title>R2:") && s.contains("<title>R1:"), "{s}");
    assert!(!s.contains("OFF") && !s.contains("BAD"), "{s}");
    // all the places in one point still make a picture with a size
    let one = plan_of(vec![placement("R1", "F1", (5.0, 5.0), (5.0, 5.0), 0, 0)]);
    let s = svg(&one, &[]);
    assert!(s.contains("<circle cx=\"48.00\" cy=\"48.00\""), "{s}");
}

#[test]
fn the_picture_keeps_to_its_size_and_leaves_the_numbers_out_when_there_are_too_many_parts() {
    let attr = |s: &str, name: &str| -> f64 {
        let tag = between(s, "<svg ", ">");
        between(tag, &format!(" {name}=\""), "\"").parse().unwrap()
    };
    // a board a hundred metres long and one millimetre wide is drawn smaller instead of taller
    let tall = plan_of(vec![
        placement("R1", "F1", (0.0, 0.0), (0.0, 0.0), 0, 0),
        placement("R2", "F1", (0.0, 0.0), (1.0, 100_000.0), 0, 0),
    ]);
    let s = svg(&tall, &[]);
    assert!(
        attr(&s, "width") <= 1000.0 && attr(&s, "height") <= 4000.0,
        "{s}"
    );
    // up to the limit the places have their numbers; past it only a dot
    let many = |n: usize| {
        plan_of(
            (0..n)
                .map(|i| {
                    placement(
                        &format!("R{i}"),
                        "F1",
                        (0.0, 0.0),
                        (i as f64, 1.0),
                        0,
                        i / 4,
                    )
                })
                .collect(),
        )
    };
    let with = svg(&many(MAX_LABELS), &[]);
    assert!(between(&with, "id=\"places\"", "</g>").contains("<text "));
    let without = svg(&many(MAX_LABELS + 1), &[]);
    let places = between(&without, "id=\"places\"", "</g>");
    assert!(!places.contains("<text ") && places.contains("r=\"2.5\""));
}

#[test]
fn schedule_writes_the_picture_once_and_never_over_a_file() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_preview_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let json = dir.join("plan.json").to_string_lossy().into_owned();
    let picture = dir.join("plan.svg").to_string_lossy().into_owned();
    let args = [
        "schedule",
        "examples/board.pos",
        "examples/feeders.csv",
        json.as_str(),
        "--packages",
        "examples/packages.csv",
        "--svg",
        picture.as_str(),
    ];
    let o = run(&args);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let s = std::fs::read_to_string(&picture).unwrap();
    assert!(
        s.starts_with("<?xml") && s.contains("<title>R1:") && s.contains("<title>F1 ("),
        "{s}"
    );
    assert!(s.contains("6 parts in "), "{s}");
    // a second time the picture is not written over, and nothing else is written either
    let json2 = dir.join("plan2.json").to_string_lossy().into_owned();
    let mut again = args;
    again[3] = json2.as_str();
    let o = run(&again);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        text(&o.stderr).contains("PHOENIX-PNP-040"),
        "{}",
        text(&o.stderr)
    );
    assert!(!PathBuf::from(&json2).exists());
    // the picture is never one of the files the plan was made from
    let o = run(&[
        "schedule",
        "examples/board.pos",
        "examples/feeders.csv",
        json.as_str(),
        "--svg",
        "examples/feeders.csv",
    ]);
    assert_eq!(o.status.code(), Some(1));
    let _ = std::fs::remove_dir_all(&dir);
}
