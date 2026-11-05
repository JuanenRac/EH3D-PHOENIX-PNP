//! The data model: error codes, lengths, the configuration file, the tables of parts and packages, the rotation corrections and the profiles of the CSV readers.

use eh3d_phoenix_pnp::catalog::{Catalog, Defaults};
use eh3d_phoenix_pnp::config::{
    ComponentOrigin, ErrorPolicy, MachineConfig, OrderStrategy, TipStrategy, KEYS, SCHEMA_VERSION,
};
use eh3d_phoenix_pnp::error::{Code, Lang, PnpError};
use eh3d_phoenix_pnp::feeding::TrayFill;
use eh3d_phoenix_pnp::fiducial::FitMode;
use eh3d_phoenix_pnp::parts::{
    from_csv, from_csv_with, BottomRotation, Part, PositionColumns, Profile, Side,
};
use eh3d_phoenix_pnp::rotation::{glob_match, rot_path, RotationTable};
use eh3d_phoenix_pnp::units::{Length, Location};

const EXAMPLE: &str = include_str!("../examples/machine.cfg");

/// A small generator, so that the generative tests give the same numbers on every run.
struct Rng(u64);

impl Rng {
    fn bits(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn unit(&mut self) -> f64 {
        (self.bits() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn between(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }
}

fn part(r: &str, v: &str, p: &str) -> Part {
    Part {
        reference: r.into(),
        value: v.into(),
        package: p.into(),
        x: 0.0,
        y: 0.0,
        rotation: 0.0,
        side: Side::Top,
    }
}

// ---- error codes

#[test]
fn every_code_has_a_number_a_label_and_a_message_in_the_eight_languages() {
    assert_eq!(Lang::ALL.len(), 8);
    for (i, code) in Code::ALL.iter().enumerate() {
        assert_eq!(code.number() as usize, i + 1, "the numbers are consecutive");
        assert_eq!(code.label(), format!("PHOENIX-PNP-{:03}", i + 1));
        for lang in Lang::ALL {
            let m = code.message(lang);
            assert!(
                !m.trim().is_empty(),
                "{} has no text in {}",
                code.label(),
                lang.code()
            );
        }
        // a translation that is the English text word for word was not translated
        for lang in &Lang::ALL[1..] {
            assert_ne!(
                code.message(*lang),
                code.message(Lang::En),
                "{} in {}",
                code.label(),
                lang.code()
            );
        }
    }
}

#[test]
fn an_error_shows_its_code_and_its_detail_and_a_localized_form() {
    let e = PnpError::new(Code::ConfigSyntax, "line 3: nonsense");
    assert_eq!(e.to_string(), "PHOENIX-PNP-003: line 3: nonsense");
    let es = e.localized(Lang::Es);
    assert!(es.starts_with("PHOENIX-PNP-003: "));
    assert!(es.contains("clave = valor"));
    assert!(es.contains("line 3: nonsense"));
    let s: String = e.into();
    assert!(s.contains("PHOENIX-PNP-003"));
    assert_eq!(Lang::from_code(" PT "), Some(Lang::Pt));
    assert_eq!(Lang::from_code("xx"), None);
}

// ---- lengths and locations

#[test]
fn lengths_are_built_from_every_unit_and_checked() {
    assert!((Length::from_mil(1000.0).unwrap().mm() - 25.4).abs() < 1e-12);
    assert!((Length::from_inch(2.0).unwrap().mm() - 50.8).abs() < 1e-12);
    assert_eq!(Length::parse(" 12.5 ").unwrap().mm(), 12.5);
    assert!((Length::parse("10mil").unwrap().mm() - 0.254).abs() < 1e-12);
    assert!((Length::parse("0.5in").unwrap().mm() - 12.7).abs() < 1e-12);
    for bad in ["", "abc", "NaN", "inf", "1e9", "5 furlongs"] {
        let e = Length::parse(bad).unwrap_err();
        assert_eq!(e.code, Code::UnitBad, "{bad}");
    }
    assert_eq!(Length::from_mm(f64::NAN).unwrap_err().code, Code::UnitBad);
    assert_eq!(Length::from_mm(10_001.0).unwrap_err().code, Code::UnitBad);
    assert!(Length::from_mm(-10_000.0).is_ok());
}

#[test]
fn a_length_converted_and_converted_back_is_the_same_length() {
    let mut rng = Rng(7);
    for _ in 0..200 {
        let v = rng.between(-300.0, 300.0);
        assert!((Length::from_mil(v).unwrap().mil() - v).abs() < 1e-9);
        assert!((Length::from_inch(v / 100.0).unwrap().inch() - v / 100.0).abs() < 1e-9);
        assert!((Length::from_mm(v).unwrap().mm() - v).abs() < 1e-12);
    }
}

#[test]
fn a_location_keeps_its_angle_in_the_circle_and_turns_about_the_origin() {
    let l = Location::from_mm(10.0, 0.0, 1.0, -90.0).unwrap();
    assert_eq!(l.rotation(), 270.0);
    let t = l.rotated_about_origin(90.0).unwrap();
    assert!(t.x.mm().abs() < 1e-9);
    assert!((t.y.mm() - 10.0).abs() < 1e-9);
    assert_eq!(t.rotation(), 0.0);
    assert_eq!(t.z.mm(), 1.0);
    assert!(Location::from_mm(0.0, 0.0, 0.0, f64::NAN).is_err());
    let a = Location::from_mm(0.0, 0.0, 0.0, 0.0).unwrap();
    let b = a.translated(3.0, 4.0).unwrap();
    assert!((a.distance_xy(&b) - 5.0).abs() < 1e-12);
    assert!(a.translated(1e9, 0.0).is_err());
}

#[test]
fn turning_a_place_forth_and_back_gives_the_same_place_and_keeps_its_distance_to_the_origin() {
    let mut rng = Rng(11);
    for _ in 0..200 {
        let l = Location::from_mm(
            rng.between(-100.0, 100.0),
            rng.between(-100.0, 100.0),
            0.0,
            rng.between(0.0, 360.0),
        )
        .unwrap();
        let d = rng.between(-720.0, 720.0);
        let there = l.rotated_about_origin(d).unwrap();
        let back = there.rotated_about_origin(-d).unwrap();
        assert!((back.x.mm() - l.x.mm()).abs() < 1e-9);
        assert!((back.y.mm() - l.y.mm()).abs() < 1e-9);
        let origin = Location::from_mm(0.0, 0.0, 0.0, 0.0).unwrap();
        assert!((there.distance_xy(&origin) - l.distance_xy(&origin)).abs() < 1e-9);
    }
}

// ---- the configuration

#[test]
fn an_empty_configuration_is_the_default_and_the_example_is_valid() {
    assert_eq!(MachineConfig::parse("").unwrap(), MachineConfig::default());
    assert_eq!(
        MachineConfig::parse("# only a comment\n\n").unwrap(),
        MachineConfig::default()
    );
    let c = MachineConfig::parse(EXAMPLE).unwrap();
    assert_eq!(c.schema_version, SCHEMA_VERSION);
    assert_eq!(c.nozzles, 2);
    assert_eq!(c.rotation_rules.len(), 2);
    assert_eq!(c.tip_changer, Some((-50.0, 0.0)));
    assert_eq!(c.order, OrderStrategy::Nearest);
    assert_eq!(c.tip_strategy, TipStrategy::Minimize);
    assert_eq!(c.error_policy, ErrorPolicy::Alert);
    assert_eq!(c.component_origin, ComponentOrigin::CadOrigin);
    // the same text twice, the same configuration
    assert_eq!(c, MachineConfig::parse(EXAMPLE).unwrap());
}

#[test]
fn the_example_names_every_key_of_the_configuration_and_nothing_else() {
    let mut in_example: Vec<String> = EXAMPLE
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| l.trim().split_once('='))
        .map(|(k, _)| k.trim().to_string())
        .collect();
    in_example.sort();
    in_example.dedup();
    let mut known: Vec<String> = KEYS.iter().map(|k| k.to_string()).collect();
    known.sort();
    known.dedup();
    assert_eq!(KEYS.len(), known.len(), "a key is listed twice");
    assert_eq!(in_example, known);
}

fn code_of(text: &str) -> Code {
    MachineConfig::parse(text).unwrap_err().code
}

#[test]
fn a_bad_configuration_is_refused_with_its_code_and_its_line() {
    assert_eq!(code_of("nonsense"), Code::ConfigSyntax);
    assert_eq!(code_of("colour = red"), Code::ConfigUnknownKey);
    assert_eq!(
        code_of("nozzles = 2\nnozzles = 3"),
        Code::ConfigDuplicateKey
    );
    assert_eq!(code_of("nozzles = two"), Code::ConfigBadValue);
    assert_eq!(code_of("nozzles = 7"), Code::ConfigOutOfRange);
    assert_eq!(code_of("nozzles = 0"), Code::ConfigOutOfRange);
    assert_eq!(code_of("pick_s = -1"), Code::ConfigOutOfRange);
    assert_eq!(code_of("pick_s = NaN"), Code::ConfigBadValue);
    assert_eq!(code_of("pre_rotate = maybe"), Code::ConfigBadValue);
    assert_eq!(code_of("language = xx"), Code::ConfigBadValue);
    assert_eq!(code_of("order = sideways"), Code::ConfigBadValue);
    assert_eq!(code_of("schema_version = 2"), Code::ConfigSchemaNewer);
    assert_eq!(code_of("rotation = QFN-*"), Code::RotationRule);
    assert_eq!(code_of("rotation = QFN-* : x"), Code::RotationRule);
    assert_eq!(code_of("rotation = QFN-* : 720"), Code::RotationRule);
    assert_eq!(code_of("tip_changer_x_mm = 5"), Code::ConfigBadValue);
    let e = MachineConfig::parse("# c\n\nnozzles = 9").unwrap_err();
    assert!(e.detail.contains("line 3"), "{}", e.detail);
    let big = "#".repeat(300_000);
    assert_eq!(code_of(&big), Code::FileTooBig);
}

#[test]
fn the_safety_limits_are_checked_together() {
    assert_eq!(code_of("z_min_mm = 10\nz_max_mm = 5"), Code::UnsafeSetting);
    assert_eq!(code_of("safe_z_mm = 60"), Code::UnsafeSetting);
    assert_eq!(
        code_of("safe_z_mm = -5\nboard_top_z_mm = 0"),
        Code::UnsafeSetting
    );
    assert_eq!(
        code_of("limit_x_min_mm = 5\nlimit_x_max_mm = 5"),
        Code::UnsafeSetting
    );
    assert_eq!(
        code_of("fault_window = 3\nfault_limit = 4"),
        Code::UnsafeSetting
    );
    assert_eq!(code_of("place_retries = 5"), Code::UnsafeSetting);
    assert!(MachineConfig::parse("place_retries = 5\nmax_placement_attempts = 6").is_ok());
    assert_eq!(
        code_of(
            "nozzle_angle_min_deg = -10\nnozzle_angle_max_deg = 0\nsafe_z_mm = 5\nz_max_mm = 1"
        ),
        Code::UnsafeSetting
    );
}

#[test]
fn the_speed_shortcut_sets_both_axes_and_the_keys_are_not_case_sensitive() {
    let c = MachineConfig::parse(
        "\u{feff}Speed_mm_s = 300\nX_SPEED_MM_S = 100\nOrder = PART\nPre_Rotate = NO",
    )
    .unwrap();
    assert_eq!(c.y_speed_mm_s, 300.0);
    assert_eq!(c.x_speed_mm_s, 100.0);
    assert_eq!(c.order, OrderStrategy::Part);
    assert!(!c.pre_rotate);
}

#[test]
fn the_rotation_rules_keep_the_order_of_the_file_and_the_headers_extend_the_reader() {
    let c = MachineConfig::parse("rotation = A* : 10\nrotation = A1 : 20\nheader_x = placement x, Pos Horizontal\nposition = origin\nbottom_rotation = mirror").unwrap();
    assert_eq!(
        c.rotation_rules,
        vec![("A*".to_string(), 10.0), ("A1".to_string(), 20.0)]
    );
    assert_eq!(
        c.profile.extra_x,
        vec!["placement x".to_string(), "pos horizontal".to_string()]
    );
    assert_eq!(c.profile.position, PositionColumns::Origin);
    assert_eq!(c.profile.bottom_rotation, BottomRotation::Mirror);
    assert_eq!(c.rotation_table().unwrap().offset_for("A1"), 10.0);
}

#[test]
fn the_defaults_of_a_part_come_from_the_configuration() {
    let c = MachineConfig::parse("default_height_mm = 2.5\ndefault_bin = 3\ndefault_tip = N75\ndefault_rank = -4\npick_retries = 5").unwrap();
    let d = c.part_defaults();
    assert_eq!(
        (d.height_mm, d.bin, d.rank, d.pick_retries),
        (2.5, 3, -4, 5)
    );
    assert_eq!(d.nozzle_tip, "N75");
}

// ---- rotation

#[test]
fn a_pattern_matches_with_stars_and_question_marks_without_regard_to_case() {
    assert!(glob_match("QFN-*", "qfn-32"));
    assert!(glob_match("*0603", "R_0603"));
    assert!(glob_match("R_??03", "r_0603"));
    assert!(glob_match("*", ""));
    assert!(glob_match("a*b*c", "aXXbYYc"));
    assert!(!glob_match("a*b*c", "aXXbYY"));
    assert!(!glob_match("QFN-?", "QFN-32"));
    assert!(!glob_match("", "x"));
    assert!(glob_match("", ""));
    assert!(glob_match("**a**", "bbbab"));
}

#[test]
fn a_pattern_without_stars_matches_only_the_same_text_and_a_star_matches_any_text() {
    let mut rng = Rng(3);
    let alphabet: Vec<char> = "abc01-_".chars().collect();
    let mut word = |n: usize| -> String {
        (0..n)
            .map(|_| alphabet[(rng.bits() % alphabet.len() as u64) as usize])
            .collect()
    };
    for _ in 0..300 {
        let (a, b) = (word(5), word(5));
        assert_eq!(glob_match(&a, &b), a == b, "{a} {b}");
        assert!(glob_match("*", &a));
        assert!(glob_match(&format!("{}*", &a[..2]), &a));
    }
}

#[test]
fn the_first_rule_that_matches_decides_and_the_offset_is_added_once() {
    let t = RotationTable::new(&[("QFN-*".to_string(), 90.0), ("*".to_string(), 180.0)]).unwrap();
    assert_eq!(t.len(), 2);
    assert_eq!(t.offset_for("qfn-32"), 90.0);
    assert_eq!(t.offset_for("R_0603"), 180.0);
    let mut parts = vec![part("U1", "MCU", "QFN-32"), part("R1", "10k", "R_0603")];
    parts[0].rotation = 300.0;
    t.apply(&mut parts);
    assert_eq!(parts[0].rotation, 30.0);
    assert_eq!(parts[1].rotation, 180.0);
    let none = RotationTable::new(&[]).unwrap();
    assert!(none.is_empty());
    assert_eq!(none.offset_for("x"), 0.0);
}

#[test]
fn a_bad_rotation_rule_is_refused() {
    assert_eq!(
        RotationTable::new(&[(" ".to_string(), 0.0)])
            .unwrap_err()
            .code,
        Code::RotationRule
    );
    assert_eq!(
        RotationTable::new(&[("a".to_string(), 361.0)])
            .unwrap_err()
            .code,
        Code::RotationRule
    );
    assert_eq!(
        RotationTable::new(&[("a".to_string(), f64::NAN)])
            .unwrap_err()
            .code,
        Code::RotationRule
    );
    let many: Vec<(String, f64)> = (0..1001).map(|i| (format!("p{i}"), 0.0)).collect();
    assert_eq!(
        RotationTable::new(&many).unwrap_err().code,
        Code::RotationRule
    );
}

#[test]
fn a_limited_nozzle_turns_to_the_angle_in_its_range_that_is_nearest() {
    assert_eq!(rot_path(0.0, 90.0, -360.0, 360.0).unwrap(), 90.0);
    assert_eq!(rot_path(0.0, 270.0, -360.0, 360.0).unwrap(), -90.0);
    assert_eq!(rot_path(350.0, 10.0, -360.0, 360.0).unwrap(), 10.0);
    assert_eq!(rot_path(0.0, 270.0, -100.0, 100.0).unwrap(), -90.0);
    assert_eq!(
        rot_path(0.0, 180.0, -100.0, 100.0).unwrap_err().code,
        Code::RotationUnreachable
    );
    assert_eq!(
        rot_path(0.0, 270.0, 0.0, 200.0).unwrap_err().code,
        Code::RotationUnreachable
    );
    assert!(rot_path(f64::NAN, 0.0, -1.0, 1.0).is_err());
    assert!(rot_path(0.0, 0.0, 5.0, -5.0).is_err());
    // two angles equally near: the lower one
    assert_eq!(rot_path(0.0, 180.0, -360.0, 360.0).unwrap(), -180.0);
}

#[test]
fn the_angle_a_nozzle_turns_to_is_in_range_the_same_direction_and_the_nearest_one() {
    let mut rng = Rng(19);
    for _ in 0..500 {
        let (from, target) = (rng.between(-720.0, 720.0), rng.between(-1000.0, 1000.0));
        let (min, max) = (rng.between(-720.0, 0.0), rng.between(0.0, 720.0));
        let valid: Vec<f64> = (-8..=8)
            .map(|k| target.rem_euclid(360.0) + 360.0 * k as f64)
            .filter(|c| (min..=max).contains(c))
            .collect();
        match rot_path(from, target, min, max) {
            Ok(c) => {
                assert!((min..=max).contains(&c));
                assert!(((c - target) / 360.0 - ((c - target) / 360.0).round()).abs() < 1e-9);
                assert!(valid
                    .iter()
                    .all(|v| (c - from).abs() <= (v - from).abs() + 1e-9));
            }
            Err(e) => {
                assert_eq!(e.code, Code::RotationUnreachable);
                assert!(valid.is_empty(), "{valid:?} are reachable");
            }
        }
    }
}

// ---- the tables of parts and packages

const PACKAGES: &str = "package,height_mm,pick_retries,bin,nozzle_tip,rank\nR_0603,0.5,3,1,N45,0\nQFN-32,0.9,,,N75,20\n";
const PARTS: &str =
    "value,package,height_mm,rank,origin_dx_mm\n10k,R_0603,0.6,5,\nMCU,QFN-32,,,0.25\n";

fn catalog() -> Catalog {
    let mut c = Catalog::new();
    c.add_packages(PACKAGES).unwrap();
    c.add_parts(PARTS).unwrap();
    c
}

#[test]
fn a_part_inherits_from_its_package_and_the_package_from_the_defaults() {
    let c = catalog();
    let d = Defaults::default();
    let r = c.resolve(&part("R1", "10k", "r_0603"), &d);
    assert_eq!(r.height_mm, 0.6, "the part wins over the package");
    assert_eq!(r.rank, 5);
    assert_eq!(
        r.pick_retries, 3,
        "the package fills what the part leaves empty"
    );
    assert_eq!(r.bin, 1);
    assert_eq!(r.nozzle_tip, "N45");
    assert_eq!(r.origin_dx_mm, 0.0, "an empty cell inherits the default");
    let m = c.resolve(&part("U1", "mcu", "QFN-32"), &d);
    assert_eq!((m.height_mm, m.rank, m.origin_dx_mm), (0.9, 20, 0.25));
    assert_eq!(
        m.pick_retries, d.pick_retries,
        "empty in both tables: the default"
    );
    assert_eq!(m.bin, d.bin);
    // a part of a package that the table of parts does not know
    let r2 = c.resolve(&part("R2", "47k", "R_0603"), &d);
    assert_eq!((r2.height_mm, r2.rank), (0.5, 0));
    // a part that nothing describes is all default
    assert_eq!(c.resolve(&part("X1", "?", "?"), &d), d);
}

#[test]
fn parts_that_no_table_describes_are_listed_and_are_not_an_error() {
    let c = catalog();
    let parts = vec![
        part("R1", "10k", "R_0603"),
        part("R2", "47k", "R_0603"),
        part("C1", "100n", "C_0402"),
        part("C2", "100N", "c_0402"),
        part("L1", "1u", "L_0805"),
    ];
    let undefined = c.undefined(&parts);
    // sorted by name; the two spellings of the same kind are two lines because the lists keep the spelling of the file
    assert_eq!(
        undefined,
        vec![
            ("100N / c_0402".to_string(), 1),
            ("100n / C_0402".to_string(), 1),
            ("1u / L_0805".to_string(), 1)
        ]
    );
    assert!(c.is_defined(&parts[1]), "the package is described");
    assert!(Catalog::new().is_empty());
    assert!(!c.is_empty());
    assert_eq!(Catalog::new().undefined(&parts[..1]).len(), 1);
}

#[test]
fn a_bad_table_is_refused_with_its_code_and_the_row() {
    let mut c = Catalog::new();
    let rows = [
        "package,colour\nR,red\n",
        "package,rank,rank\nR,1,2\n",
        "height_mm\n1\n",
        "package,height_mm\nR,tall\n",
        "package,height_mm\nR,101\n",
        "package,height_mm\nR,NaN\n",
        "package,pick_retries\nR,1.5\n",
        "package,bin\nR,5\n",
        "package,rank\nR,1,2\n",
        "",
    ];
    for text in rows {
        let e = c.add_packages(text).unwrap_err();
        assert_eq!(e.code, Code::CatalogBadRow, "{text:?}: {}", e.detail);
    }
    let e = c.add_packages("package,rank\nR,1\nS,x\n").unwrap_err();
    assert!(e.detail.contains("row 3"), "{}", e.detail);
    assert_eq!(
        c.add_packages("package\nR\nr\n").unwrap_err().code,
        Code::CatalogDuplicate
    );
    assert_eq!(
        c.add_parts("value,package\n10k,R\n10K,r\n")
            .unwrap_err()
            .code,
        Code::CatalogDuplicate
    );
}

#[test]
fn a_table_may_use_semicolons_quotes_and_a_byte_order_mark() {
    let mut c = Catalog::new();
    c.add_parts("\u{feff}value;package;nozzle_tip\n\"10k, 1%\";R_0603;\"N 45\"\n")
        .unwrap();
    let r = c.resolve(&part("R1", "10k, 1%", "R_0603"), &Defaults::default());
    assert_eq!(r.nozzle_tip, "N 45");
}

// ---- the readers of the position file

const ALTIUM: &str = "Designator,Footprint,Mid X,Mid Y,Ref X,Ref Y,Layer,Rotation,Comment\nR1,R_0603,10mm,20mm,9mm,19mm,TopLayer,90,10k\nR2,R_0603,100mil,200mil,50mil,150mil,BottomLayer,30,10k\n";

#[test]
fn a_file_with_the_centre_and_the_origin_is_read_with_the_pair_the_profile_prefers() {
    let any = from_csv(ALTIUM).unwrap();
    assert_eq!(
        (any[0].x, any[0].y),
        (10.0, 20.0),
        "the first pair of the file"
    );
    let centre = from_csv_with(
        ALTIUM,
        &Profile {
            position: PositionColumns::Center,
            ..Profile::default()
        },
    )
    .unwrap();
    assert_eq!(centre[0].x, 10.0);
    let origin = from_csv_with(
        ALTIUM,
        &Profile {
            position: PositionColumns::Origin,
            ..Profile::default()
        },
    )
    .unwrap();
    assert_eq!((origin[0].x, origin[0].y), (9.0, 19.0));
    assert!(
        (origin[1].x - 1.27).abs() < 1e-9 && (origin[1].y - 3.81).abs() < 1e-9,
        "mil converted"
    );
    assert_eq!(any[0].value, "10k");
    assert_eq!(any[1].side, Side::Bottom);
}

#[test]
fn the_bottom_side_rotation_rule_changes_only_the_parts_of_the_bottom() {
    let read = |b| {
        from_csv_with(
            ALTIUM,
            &Profile {
                bottom_rotation: b,
                ..Profile::default()
            },
        )
        .unwrap()
    };
    let keep = read(BottomRotation::Keep);
    let mirror = read(BottomRotation::Mirror);
    let half = read(BottomRotation::HalfTurn);
    assert_eq!((keep[0].rotation, keep[1].rotation), (90.0, 30.0));
    assert_eq!((mirror[0].rotation, mirror[1].rotation), (90.0, 330.0));
    assert_eq!((half[0].rotation, half[1].rotation), (90.0, 210.0));
}

#[test]
fn the_synonyms_of_the_configuration_name_more_columns() {
    let text = "Item;Part Number;Housing;Horizontal;Vertical;Turn;Where\nR1;10k;R_0603;1;2;0;t\n";
    assert!(from_csv(text).is_err());
    let profile = Profile {
        extra_reference: vec!["Item".to_string()],
        extra_value: vec!["part number".to_string()],
        extra_package: vec!["housing".to_string()],
        extra_x: vec!["horizontal".to_string()],
        extra_y: vec!["vertical".to_string()],
        extra_rotation: vec!["turn".to_string()],
        extra_side: vec!["where".to_string()],
        ..Profile::default()
    };
    let parts = from_csv_with(text, &profile).unwrap();
    assert_eq!(parts.len(), 1);
    assert_eq!(
        (parts[0].reference.as_str(), parts[0].x, parts[0].y),
        ("R1", 1.0, 2.0)
    );
}

#[test]
fn spaces_around_the_names_are_never_part_of_them() {
    let parts =
        from_csv("ref,val,package,x,y,rot,side\n  R1  ,  10k  ,  R_0603 ,1,2,0,top\n").unwrap();
    assert_eq!(parts[0].reference, "R1");
    assert_eq!(parts[0].value, "10k");
    assert_eq!(parts[0].package, "R_0603");
    assert_eq!(parts[0].key(), "10k|r_0603");
}
// ---- the keys of the fiducials, the trays, the undo, the search for the order of the tips and the failures by package

#[test]
fn the_new_keys_are_read_and_checked() {
    let d = MachineConfig::default();
    assert_eq!(d.fiducial_mode, FitMode::Auto);
    assert_eq!(d.fiducial_max_residual_mm, 0.5);
    assert_eq!(d.tray_fill, TrayFill::Rows);
    assert_eq!(d.undo_limit, 100);
    assert_eq!(d.tip_opt_max_evals, 20_000);
    assert_eq!(d.tip_opt_time_limit_s, 1.0);
    assert!(d.sim_part_fail.is_empty() && d.sim_feeder_fail.is_empty());
    let c = MachineConfig::parse(
        "fiducial_mode = Affine\nfiducial_max_residual_mm = 0.25\ntray_fill = SNAKE\nundo_limit = 5\n\
         tip_strategy = optimize\ntip_opt_max_evals = 10\ntip_opt_time_limit_s = 0.5\n\
         sim_part_fail = QFN-* : 0.1 : 0.05\nsim_part_fail = R* : 0 : 1\nsim_feeder_fail = F2 : 0.25\nsim_feeder_fail = F9 : 1",
    )
    .unwrap();
    assert_eq!(c.fiducial_mode, FitMode::Affine);
    assert_eq!(c.fiducial_max_residual_mm, 0.25);
    assert_eq!(c.tray_fill, TrayFill::Snake);
    assert_eq!(c.undo_limit, 5);
    assert_eq!(c.tip_strategy, TipStrategy::Optimize);
    assert_eq!((c.tip_opt_max_evals, c.tip_opt_time_limit_s), (10, 0.5));
    // the rules keep the order of the file and may be written more than once
    assert_eq!(
        c.sim_part_fail,
        vec![
            ("QFN-*".to_string(), 0.1, 0.05),
            ("R*".to_string(), 0.0, 1.0)
        ]
    );
    assert_eq!(
        c.sim_feeder_fail,
        vec![("F2".to_string(), 0.25), ("F9".to_string(), 1.0)]
    );
    for (text, code) in [
        ("fiducial_mode = loose", Code::ConfigBadValue),
        ("fiducial_max_residual_mm = 0", Code::ConfigOutOfRange),
        ("fiducial_max_residual_mm = 101", Code::ConfigOutOfRange),
        ("tray_fill = diagonal", Code::ConfigBadValue),
        ("undo_limit = 0", Code::ConfigOutOfRange),
        ("undo_limit = 1001", Code::ConfigOutOfRange),
        ("tip_strategy = best", Code::ConfigBadValue),
        ("tip_opt_max_evals = 0", Code::ConfigOutOfRange),
        ("tip_opt_time_limit_s = 100", Code::ConfigOutOfRange),
        ("tip_opt_time_limit_s = 0", Code::ConfigOutOfRange),
        ("sim_part_fail = QFN", Code::ConfigBadValue),
        ("sim_part_fail = QFN : 0.1", Code::ConfigBadValue),
        ("sim_part_fail = : 0.1 : 0.1", Code::ConfigBadValue),
        ("sim_part_fail = QFN : 2 : 0", Code::ConfigOutOfRange),
        ("sim_part_fail = QFN : 0 : -0.1", Code::ConfigOutOfRange),
        ("sim_part_fail = QFN : x : 0", Code::ConfigBadValue),
        ("sim_feeder_fail = F1", Code::ConfigBadValue),
        ("sim_feeder_fail = F1 : 0.1 : 0.1", Code::ConfigBadValue),
        ("sim_feeder_fail = F1 : 1.5", Code::ConfigOutOfRange),
        (
            "fiducial_mode = auto\nfiducial_mode = auto",
            Code::ConfigDuplicateKey,
        ),
    ] {
        assert_eq!(code_of(text), code, "{text}");
    }
}

#[test]
fn the_rules_of_the_bottom_side_give_the_angles_worked_out_by_hand() {
    // (angle in the file, mirror, half turn)
    let cases = [
        (0.0, 0.0, 180.0),
        (30.0, 330.0, 210.0),
        (180.0, 180.0, 0.0),
        (270.0, 90.0, 90.0),
        (359.0, 1.0, 179.0),
    ];
    for (file, mirror, half) in cases {
        assert_eq!(BottomRotation::Keep.apply(file), file);
        assert_eq!(BottomRotation::Mirror.apply(file), mirror, "mirror {file}");
        assert_eq!(
            BottomRotation::HalfTurn.apply(file),
            half,
            "half turn {file}"
        );
    }
    let names: Vec<&str> = BottomRotation::ALL.iter().map(|r| r.name()).collect();
    assert_eq!(names, ["keep", "mirror", "half_turn"]);
    // the reader of a CSV applies the rule of its profile to the bottom side and to nothing else
    let csv = "ref,val,package,x,y,rot,side\nC5,100n,C,1,2,30,bottom\nR1,10k,R,3,4,30,top\n";
    for (rule, expected) in [
        (BottomRotation::Keep, 30.0),
        (BottomRotation::Mirror, 330.0),
        (BottomRotation::HalfTurn, 210.0),
    ] {
        let profile = Profile {
            bottom_rotation: rule,
            ..Profile::default()
        };
        let parts = from_csv_with(csv, &profile).unwrap();
        assert_eq!(parts[0].rotation, expected, "{}", rule.name());
        assert_eq!(parts[1].rotation, 30.0);
    }
    // the table of the parts of the bottom side
    use eh3d_phoenix_pnp::parts::bottom_table;
    assert_eq!(
        bottom_table(&[], BottomRotation::Keep),
        "0 of 0 parts are on the bottom side; the rule in use is keep\n"
    );
    let parts = from_csv(csv).unwrap();
    let table = bottom_table(&parts, BottomRotation::HalfTurn);
    assert_eq!(
        table.lines().count(),
        3,
        "the summary, the head and the one part of the bottom"
    );
    assert!(
        table.starts_with("1 of 2 parts are on the bottom side; the rule in use is half_turn\n")
    );
    let row: Vec<&str> = table.lines().last().unwrap().split_whitespace().collect();
    assert_eq!(
        row,
        ["C5", "100n", "C", "30.00", "330.00", "210.00", "210.00"]
    );
}
