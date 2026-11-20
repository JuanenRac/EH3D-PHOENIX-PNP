//! The checks of a board for assembly (spaces, pads, holes, tracks, fiducials, tooling holes, tabs) with the limits in the settings, the report in the eight languages, and the openings of the
//! paste stencil made from the pads: the reduction, the windows of a thermal pad, the area ratio and the thickness.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::dfm::{
    check, message, parse_features, Feature, Finding, Kind, Limits, Rule, Severity,
};
use eh3d_phoenix_pnp::error::{Code, Lang};
use eh3d_phoenix_pnp::parts::{Part, Side};
use eh3d_phoenix_pnp::stencil::{self, area_ratio, Settings};

fn limits() -> Limits {
    Limits::from_config(&MachineConfig::default())
}

fn feature(kind: Kind, id: &str, x: f64, y: f64, w: f64, h: f64) -> Feature {
    Feature {
        kind,
        id: id.into(),
        x,
        y,
        w,
        h,
        side: Side::Top,
    }
}

fn outline() -> Feature {
    feature(Kind::Outline, "board", 0.0, 0.0, 100.0, 80.0)
}

fn part(reference: &str, package: &str, x: f64, y: f64) -> Part {
    Part {
        reference: reference.into(),
        value: "10k".into(),
        package: package.into(),
        x,
        y,
        rotation: 0.0,
        side: Side::Top,
    }
}

fn catalog() -> Catalog {
    let mut c = Catalog::new();
    c.add_packages("package,body_long_mm,body_short_mm\nR_0603,2,1\nPinHeader_1x02,5,2\n")
        .unwrap();
    c
}

fn rules(features: &[Feature], parts: &[Part]) -> Vec<Rule> {
    check(parts, features, &catalog(), &limits())
        .findings
        .iter()
        .map(|f| f.rule)
        .collect()
}

#[test]
fn a_clean_board_has_no_finding_and_the_report_says_so_in_one_line() {
    let features = vec![
        outline(),
        feature(Kind::Pad, "R1", 50.0, 40.0, 1.0, 1.0),
        feature(Kind::Pad, "R1", 54.0, 40.0, 1.0, 1.0),
        feature(Kind::Tooling, "T1", 10.0, 10.0, 2.0, 2.0),
        feature(Kind::Fiducial, "F1", 20.0, 20.0, 1.0, 1.0),
        feature(Kind::Tab, "tab", 0.0, 0.0, 5.0, 5.0),
    ];
    let parts = vec![part("R1", "R_0603", 52.0, 40.0)];
    let r = check(&parts, &features, &catalog(), &limits());
    assert!(r.findings.is_empty(), "{:?}", r.findings);
    assert_eq!(r.problems(), 0);
    assert!(r
        .text(Lang::En)
        .contains("0 findings: 0 errors, 0 warnings, 0 notes (1 parts, 2 pads)"));
}

#[test]
fn pads_at_a_fine_pitch_are_a_note_and_not_a_warning() {
    let features = vec![
        feature(Kind::Pad, "U1", 10.0, 10.0, 0.3, 0.8),
        feature(Kind::Pad, "U1", 10.5, 10.0, 0.3, 0.8),
        feature(Kind::Pad, "R1", 20.0, 10.0, 1.0, 1.0),
        feature(Kind::Pad, "R1", 23.0, 10.0, 1.0, 1.0),
    ];
    let r = check(&[], &features, &catalog(), &limits());
    assert_eq!(r.findings.len(), 1);
    let f = &r.findings[0];
    assert_eq!((f.rule, f.subject.as_str()), (Rule::PadPitch, "U1"));
    assert!((f.value - 0.5).abs() < 1e-9 && (f.limit - 1.27).abs() < 1e-9);
    assert_eq!(f.rule.severity(), Severity::Note);
    assert_eq!(r.problems(), 0);
}

#[test]
fn the_gap_between_the_pads_of_two_parts_is_told_when_it_is_under_the_limit() {
    let features = vec![
        feature(Kind::Pad, "R1", 20.0, 20.0, 1.0, 1.0),
        feature(Kind::Pad, "R2", 21.1, 20.0, 1.0, 1.0),
        feature(Kind::Pad, "R3", 40.0, 20.0, 1.0, 1.0),
        feature(Kind::Pad, "R4", 41.5, 20.0, 1.0, 1.0),
    ];
    let r = check(&[], &features, &catalog(), &limits());
    let gaps: Vec<&Finding> = r
        .findings
        .iter()
        .filter(|f| f.rule == Rule::PadGap)
        .collect();
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].subject, "R1 / R2");
    assert!((gaps[0].value - 0.1).abs() < 1e-9);
}

#[test]
fn pads_on_the_two_sides_do_not_count_against_each_other() {
    let mut other = feature(Kind::Pad, "R2", 21.1, 20.0, 1.0, 1.0);
    other.side = Side::Bottom;
    let features = vec![feature(Kind::Pad, "R1", 20.0, 20.0, 1.0, 1.0), other];
    assert!(!rules(&features, &[]).contains(&Rule::PadGap));
}

#[test]
fn a_hole_inside_a_pad_that_is_soldered_is_an_error() {
    let features = vec![
        feature(Kind::Pad, "C1", 30.0, 30.0, 1.2, 1.2),
        feature(Kind::Hole, "via", 30.2, 30.1, 0.3, 0.3),
        feature(Kind::Hole, "far", 35.0, 30.0, 0.3, 0.3),
    ];
    let r = check(&[], &features, &catalog(), &limits());
    assert_eq!(r.findings.len(), 1);
    assert_eq!(r.findings[0].rule, Rule::HoleInPad);
    assert_eq!(r.findings[0].rule.severity(), Severity::Error);
    assert_eq!(r.problems(), 1);
}

#[test]
fn a_track_that_ends_in_the_middle_of_a_pad_is_told() {
    let features = vec![
        feature(Kind::Pad, "R1", 10.0, 10.0, 1.0, 1.0),
        feature(Kind::Track, "n1", 10.05, 10.0, 0.2, 0.2),
        feature(Kind::Track, "n2", 10.45, 10.0, 0.2, 0.2),
    ];
    let r = check(&[], &features, &catalog(), &limits());
    assert_eq!(r.findings.len(), 1);
    assert_eq!(r.findings[0].rule, Rule::TrackCenter);
    assert!((r.findings[0].limit - 0.125).abs() < 1e-9);
}

#[test]
fn the_space_between_parts_on_the_surface_is_judged_and_a_part_through_the_board_is_left_out() {
    let parts = vec![
        part("R1", "R_0603", 10.0, 10.0),
        part("R2", "R_0603", 12.5, 10.0),
        part("R3", "R_0603", 30.0, 10.0),
        part("J1", "PinHeader_1x02", 31.5, 10.0),
    ];
    let r = check(&parts, &[], &catalog(), &limits());
    assert_eq!(r.findings.len(), 1, "{:?}", r.findings);
    assert_eq!(r.findings[0].rule, Rule::SmtGap);
    assert_eq!(r.findings[0].subject, "R1 / R2");
    assert!((r.findings[0].value - 0.5).abs() < 1e-9);
}

#[test]
fn the_tooling_holes_are_judged_by_their_size_their_edge_and_the_parts_near_that_edge() {
    let features = vec![
        outline(),
        feature(Kind::Tooling, "T1", 10.0, 2.0, 2.5, 2.5),
        feature(Kind::Tooling, "T2", 90.0, 40.0, 2.0, 2.0),
    ];
    let parts = vec![
        part("R1", "R_0603", 50.0, 4.0),
        part("R2", "R_0603", 50.0, 30.0),
    ];
    let found = rules(&features, &parts);
    assert!(found.contains(&Rule::ToolingSize));
    assert!(found.contains(&Rule::ToolingEdge));
    // R1 is 4 mm from the bottom edge, which has a tooling hole; R2 is 30 mm from it and 50 from the others
    let r = check(&parts, &features, &catalog(), &limits());
    let near: Vec<&str> = r
        .findings
        .iter()
        .filter(|f| f.rule == Rule::SmtToolingEdge)
        .map(|f| f.subject.as_str())
        .collect();
    assert_eq!(near, ["R1"]);
}

#[test]
fn the_fiducials_are_judged_by_their_size_their_edge_and_the_ring_that_is_kept_free() {
    let features = vec![
        outline(),
        feature(Kind::Fiducial, "F1", 2.0, 40.0, 1.5, 1.5),
        feature(Kind::Fiducial, "F2", 50.0, 40.0, 1.0, 1.0),
        feature(Kind::Pad, "R1", 51.0, 40.0, 0.5, 0.5),
        feature(Kind::Fiducial, "F3", 90.0, 70.0, 1.0, 1.0),
    ];
    let r = check(&[], &features, &catalog(), &limits());
    let of = |rule: Rule| -> Vec<&str> {
        r.findings
            .iter()
            .filter(|f| f.rule == rule)
            .map(|f| f.subject.as_str())
            .collect()
    };
    assert_eq!(of(Rule::FiducialSize), ["F1"]);
    assert_eq!(of(Rule::FiducialEdge), ["F1"]);
    assert_eq!(of(Rule::FiducialRing), ["F2"]);
}

#[test]
fn a_figure_exactly_at_its_limit_keeps_it_even_when_the_sum_is_not_exact() {
    let features = vec![
        outline(),
        feature(Kind::Fiducial, "F2", 96.15, 3.85, 1.0, 1.0),
        feature(Kind::Tab, "left", 0.0, 0.0, 4.0, 80.0),
        feature(Kind::Tooling, "T1", 3.0, 40.0, 2.0, 2.0),
    ];
    assert!(rules(&features, &[]).is_empty());
}

#[test]
fn a_tab_that_is_too_narrow_is_told() {
    let features = vec![
        feature(Kind::Tab, "left", 0.0, 0.0, 3.0, 80.0),
        feature(Kind::Tab, "right", 97.0, 0.0, 4.0, 80.0),
    ];
    let r = check(&[], &features, &catalog(), &limits());
    let tabs: Vec<&str> = r
        .findings
        .iter()
        .filter(|f| f.rule == Rule::TabWidth)
        .map(|f| f.subject.as_str())
        .collect();
    assert_eq!(tabs, ["left"]);
}

#[test]
fn without_an_outline_the_edge_checks_are_left_out_and_the_report_says_so() {
    let features = vec![feature(Kind::Fiducial, "F1", 1.0, 1.0, 1.0, 1.0)];
    let r = check(&[], &features, &catalog(), &limits());
    assert!(r.findings.iter().any(|f| f.rule == Rule::NoOutline));
    assert!(!r.findings.iter().any(|f| f.rule == Rule::FiducialEdge));
    assert_eq!(r.problems(), 0);
}

#[test]
fn the_errors_come_first_then_the_warnings_then_the_notes() {
    let features = vec![
        feature(Kind::Pad, "U1", 10.0, 10.0, 0.3, 0.8),
        feature(Kind::Pad, "U1", 10.5, 10.0, 0.3, 0.8),
        feature(Kind::Pad, "C1", 30.0, 30.0, 1.2, 1.2),
        feature(Kind::Hole, "via", 30.0, 30.0, 0.3, 0.3),
        feature(Kind::Tab, "t", 0.0, 0.0, 1.0, 1.0),
        feature(Kind::Fiducial, "F", 1.0, 1.0, 1.0, 1.0),
    ];
    let r = check(&[], &features, &catalog(), &limits());
    let order: Vec<Severity> = r.findings.iter().map(|f| f.rule.severity()).collect();
    let mut sorted = order.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(order, sorted);
    assert_eq!(order[0], Severity::Error);
}

#[test]
fn the_message_of_every_rule_is_in_the_eight_languages_with_nothing_left_unfilled() {
    for rule in Rule::ALL {
        let f = Finding {
            rule,
            subject: "U9".into(),
            value: 0.5,
            limit: 1.27,
            x: 1.0,
            y: 2.0,
        };
        let mut seen = std::collections::BTreeSet::new();
        for lang in Lang::ALL {
            let m = message(&f, lang);
            assert!(
                !m.contains('{') && !m.contains('}'),
                "{rule:?} {lang:?}: {m}"
            );
            assert!(!m.is_empty());
            assert!(
                seen.insert(m.clone()),
                "{rule:?} {lang:?} repeats a language"
            );
            if rule != Rule::NoOutline {
                assert!(m.contains("U9") || rule == Rule::NoOutline, "{m}");
            }
            if !matches!(rule, Rule::NoOutline | Rule::HoleInPad) {
                assert!(m.contains("0.5"), "{rule:?} {lang:?}: {m}");
            }
        }
        assert_eq!(seen.len(), 8);
    }
}

#[test]
fn the_report_says_that_the_limits_are_not_verified_until_the_setting_says_so() {
    let r = check(&[], &[outline()], &catalog(), &limits());
    for lang in Lang::ALL {
        assert!(r.text(lang).contains("dfm_verified = off"), "{lang:?}");
    }
    let mut verified = limits();
    verified.verified = true;
    let r = check(&[], &[outline()], &catalog(), &verified);
    for lang in Lang::ALL {
        let t = r.text(lang);
        assert!(
            t.contains("dfm_verified = on") && !t.contains("dfm_verified = off"),
            "{lang:?}"
        );
    }
}

#[test]
fn the_csv_has_the_message_in_the_language_asked_for() {
    let features = vec![
        feature(Kind::Pad, "R1", 20.0, 20.0, 1.0, 1.0),
        feature(Kind::Pad, "R2", 21.1, 20.0, 1.0, 1.0),
    ];
    let r = check(&[], &features, &catalog(), &limits());
    let en = r.csv(Lang::En);
    assert!(en.starts_with("severity,rule,subject,value_mm,limit_mm,x_mm,y_mm,message\n"));
    assert!(en.contains("warning,pad_gap,R1 / R2,0.1,0.2,20.0000,20.0000,"));
    let ja = r.csv(Lang::Ja);
    assert!(ja.contains("パッド間の隙間"));
    let pt = r.csv(Lang::Pt);
    assert!(pt.contains("o espaço entre pads"));
}

#[test]
fn the_table_of_features_is_read_by_the_names_of_its_columns_and_a_row_that_makes_no_sense_is_named(
) {
    let ok = "kind,id,x_mm,y_mm,w_mm\noutline,b,0,0,10\npad,R1,1,2,0.5\nfiducial,F1,3,3,1\n";
    let f = parse_features(ok).unwrap();
    assert_eq!(f.len(), 3);
    assert_eq!((f[1].w, f[1].h, f[1].side), (0.5, 0.5, Side::Top));
    let with_side = "type;ref;x;y;d;h;layer\npad;R1;1;2;0.5;0.8;bottom\n";
    let f = parse_features(with_side).unwrap();
    assert_eq!((f[0].h, f[0].side), (0.8, Side::Bottom));
    for (text, line, word) in [
        ("kind,id,x_mm,y_mm,w_mm\nwidget,R1,1,2,3\n", 2, "kind is"),
        ("kind,id,x_mm,y_mm,w_mm\npad,R1,x,2,3\n", 2, "x_mm"),
        ("kind,id,x_mm,y_mm,w_mm\npad,R1,1,2,-3\n", 2, "w_mm"),
        (
            "kind,id,x_mm,y_mm,w_mm,side\npad,R1,1,2,3,left\n",
            2,
            "side",
        ),
        ("kind,id,x_mm,y_mm\npad,R1,1,2\n", 1, "w_mm"),
        ("", 1, "empty"),
    ] {
        let e = parse_features(text).unwrap_err();
        assert_eq!(e.code, Code::ImportBad, "{text:?}");
        assert!(
            e.to_string().contains(&format!("line {line}")) && e.to_string().contains(word),
            "{text:?}: {e}"
        );
    }
}

#[test]
fn the_limits_are_settings_with_the_public_figures_as_their_defaults() {
    let d = MachineConfig::default();
    assert_eq!(
        [
            d.dfm_pad_pitch_mm,
            d.dfm_smt_gap_mm,
            d.dfm_tooling_edge_mm,
            d.dfm_smt_tooling_edge_mm,
            d.dfm_tooling_hole_mm,
            d.dfm_fid_diameter_mm,
            d.dfm_fid_edge_mm,
            d.dfm_rail_min_mm,
        ],
        [1.27, 1.0, 3.0, 5.0, 2.0, 1.0, 3.85, 4.0]
    );
    assert!(!d.dfm_verified);
    for k in KEYS.iter().filter(|k| k.starts_with("dfm_")) {
        assert!(
            MachineConfig::parse(&format!("{k} = 99999")).is_err(),
            "{k}"
        );
    }
    let c = MachineConfig::parse("dfm_smt_gap_mm = 0.5\ndfm_verified = yes\n").unwrap();
    let l = Limits::from_config(&c);
    assert!((l.smt_gap - 0.5).abs() < 1e-12 && l.verified);
    let e = MachineConfig::parse("dfm_track_center_frac = 1.5\n").unwrap_err();
    assert!(e.to_string().contains("dfm_track_center_frac"));
}

// ---- the stencil

fn settings() -> Settings {
    Settings::from_config(&MachineConfig::default())
}

#[test]
fn the_area_ratio_is_the_area_of_the_opening_over_the_area_of_its_walls() {
    assert!((area_ratio(1.0, 1.0, 0.12) - 1.0 / (4.0 * 0.12)).abs() < 1e-12);
    assert_eq!(area_ratio(0.0, 1.0, 0.12), 0.0);
    assert_eq!(area_ratio(1.0, 1.0, 0.0), 0.0);
    assert!(area_ratio(0.2, 0.2, 0.12) < 0.66);
}

#[test]
fn an_opening_is_smaller_than_its_pad_by_the_reduction_of_each_side() {
    let features = vec![
        feature(Kind::Pad, "R1", 10.0, 10.0, 1.0, 1.0),
        feature(Kind::Pad, "R1", 14.0, 10.0, 1.0, 1.0),
    ];
    let s = stencil::make(&features, &settings());
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
    assert_eq!(s.apertures.len(), 2);
    let a = &s.apertures[0];
    assert!((a.w - 0.94).abs() < 1e-9 && (a.h - 0.94).abs() < 1e-9);
    assert_eq!((a.kind, a.reduction), ("pad", 6.0));
    assert!((a.area_ratio - area_ratio(0.94, 0.94, 0.12)).abs() < 1e-12);
}

#[test]
fn the_pads_of_a_fine_pitch_are_reduced_more() {
    let features = vec![
        feature(Kind::Pad, "U1", 10.0, 10.0, 0.3, 1.0),
        feature(Kind::Pad, "U1", 10.5, 10.0, 0.3, 1.0),
        feature(Kind::Pad, "R1", 30.0, 10.0, 1.0, 1.0),
    ];
    let s = stencil::make(&features, &settings());
    let of = |id: &str| s.apertures.iter().find(|a| a.id == id).unwrap();
    assert_eq!(of("U1").reduction, 15.0);
    assert!((of("U1").w - 0.255).abs() < 1e-9);
    assert_eq!(of("R1").reduction, 6.0);
}

#[test]
fn a_thermal_pad_is_split_into_windows_that_cover_the_share_of_the_settings() {
    let features = vec![feature(Kind::Pad, "U1", 20.0, 20.0, 3.0, 3.0)];
    let s = stencil::make(&features, &settings());
    assert_eq!(s.apertures.len(), 4);
    assert!(s.apertures.iter().all(|a| a.kind == "window"));
    let covered: f64 = s.apertures.iter().map(|a| a.w * a.h).sum();
    assert!((covered / 9.0 - 0.6).abs() < 1e-9, "{covered}");
    for a in &s.apertures {
        assert!(a.x - a.w / 2.0 >= 18.5 - 1e-9 && a.x + a.w / 2.0 <= 21.5 + 1e-9);
        assert!(a.y - a.h / 2.0 >= 18.5 - 1e-9 && a.y + a.h / 2.0 <= 21.5 + 1e-9);
    }
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
}

#[test]
fn a_cover_out_of_range_and_a_web_that_is_too_narrow_are_warned() {
    let cfg = MachineConfig {
        stencil_thermal_percent: 95.0,
        ..MachineConfig::default()
    };
    let features = vec![feature(Kind::Pad, "U1", 20.0, 20.0, 3.0, 3.0)];
    let s = stencil::make(&features, &Settings::from_config(&cfg));
    let found: Vec<stencil::Rule> = s.warnings.iter().map(|w| w.rule).collect();
    assert!(found.contains(&stencil::Rule::ThermalRange));
    assert!(found.contains(&stencil::Rule::WebNarrow));
    let range = s
        .warnings
        .iter()
        .find(|w| w.rule == stencil::Rule::ThermalRange)
        .unwrap();
    assert_eq!((range.value, range.limit), (95.0, 75.0));
}

#[test]
fn the_thickness_and_the_reductions_are_checked_against_their_ranges() {
    let cfg = MachineConfig {
        stencil_thickness_mm: 0.2,
        stencil_reduction_percent: 2.0,
        stencil_fine_reduction_percent: 30.0,
        ..MachineConfig::default()
    };
    let s = stencil::make(&[], &Settings::from_config(&cfg));
    let found: Vec<stencil::Rule> = s.warnings.iter().map(|w| w.rule).collect();
    assert_eq!(
        found,
        [
            stencil::Rule::ThicknessRange,
            stencil::Rule::ReductionRange,
            stencil::Rule::FineReductionRange
        ]
    );
    assert_eq!(s.warnings[0].limit, 0.15);
    assert_eq!(s.warnings[1].limit, 5.0);
    assert_eq!(s.warnings[2].limit, 20.0);
}

#[test]
fn an_opening_whose_area_ratio_is_too_small_is_warned() {
    let features = vec![
        feature(Kind::Pad, "T1", 5.0, 5.0, 0.2, 0.2),
        feature(Kind::Pad, "R1", 15.0, 5.0, 0.6, 0.6),
    ];
    let s = stencil::make(&features, &settings());
    let small: Vec<&str> = s
        .warnings
        .iter()
        .filter(|w| w.rule == stencil::Rule::AreaRatio)
        .map(|w| w.subject.as_str())
        .collect();
    assert_eq!(small, ["T1"]);
}

#[test]
fn the_stencil_report_and_the_csv_are_in_the_language_asked_for() {
    let features = vec![
        feature(Kind::Pad, "T1", 5.0, 5.0, 0.2, 0.2),
        feature(Kind::Pad, "U1", 20.0, 20.0, 3.0, 3.0),
    ];
    let s = stencil::make(&features, &settings());
    let mut seen = std::collections::BTreeSet::new();
    for lang in Lang::ALL {
        let t = s.text(lang);
        assert!(!t.contains('{'), "{lang:?}: {t}");
        assert!(seen.insert(t));
    }
    assert!(s
        .text(Lang::En)
        .ends_with("5 openings (4 of them windows of thermal pads), 1 warnings\n"));
    let csv = s.csv();
    assert!(csv.starts_with("id,kind,x_mm,y_mm,w_mm,h_mm,area_ratio,reduction_percent\n"));
    assert_eq!(csv.lines().count(), 6);
    assert!(csv.contains("U1,window,"));
}

#[test]
fn the_openings_made_on_a_fixed_seed_stay_inside_their_pads_and_are_made_for_every_pad() {
    let mut x: u64 = 0x57E2C11;
    let mut next = || {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (x >> 33) as f64 / (1u64 << 31) as f64
    };
    for round in 0..300 {
        let n = 1 + (next() * 12.0) as usize;
        let features: Vec<Feature> = (0..n)
            .map(|i| {
                feature(
                    Kind::Pad,
                    &format!("P{}", i % 4),
                    next() * 90.0,
                    next() * 70.0,
                    0.1 + next() * 5.0,
                    0.1 + next() * 5.0,
                )
            })
            .collect();
        let s = stencil::make(&features, &settings());
        assert!(!s.apertures.is_empty(), "round {round}");
        for a in &s.apertures {
            assert!(
                a.w > 0.0 && a.h > 0.0 && a.area_ratio.is_finite(),
                "round {round}: {a:?}"
            );
        }
        let pad_area: f64 = features.iter().map(|f| f.w * f.h).sum();
        let open_area: f64 = s.apertures.iter().map(|a| a.w * a.h).sum();
        assert!(open_area < pad_area, "round {round}");
    }
}

#[test]
fn the_settings_of_the_stencil_are_keys_with_the_figures_of_the_guides() {
    let d = MachineConfig::default();
    assert_eq!(
        [
            d.stencil_thickness_min_mm,
            d.stencil_thickness_max_mm,
            d.stencil_reduction_min_percent,
            d.stencil_reduction_max_percent,
            d.stencil_fine_reduction_min_percent,
            d.stencil_fine_reduction_max_percent,
            d.stencil_thermal_min_percent,
            d.stencil_thermal_max_percent,
            d.stencil_area_ratio_min,
        ],
        [0.10, 0.15, 5.0, 7.0, 10.0, 20.0, 50.0, 75.0, 0.66]
    );
    for k in KEYS.iter().filter(|k| k.starts_with("stencil_")) {
        assert!(
            MachineConfig::parse(&format!("{k} = 99999")).is_err(),
            "{k}"
        );
    }
    let e = MachineConfig::parse("stencil_thickness_mm = 0.9\n").unwrap_err();
    assert_eq!(e.code, Code::ConfigOutOfRange);
    assert!(e.to_string().contains("stencil_thickness_mm"));
}
