//! The read-only import of the configuration files of another program: the bounded XML reader (limits, no entities, never a panic), the units, the mapping of packages, parts and feeders,
//! the findings with their file and line, and the files damaged on a fixed seed. The fixtures are written for these tests; none is a file of a real machine.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::feeders::from_csv as feeders_csv;
use eh3d_phoenix_pnp::openpnp::{message, unit_factor, Imported, Issue, Severity};
use eh3d_phoenix_pnp::xmlmini::{
    parse, Problem, MAX_ATTRIBUTES, MAX_BYTES, MAX_DEPTH, MAX_ELEMENTS, MAX_NAME, MAX_VALUE,
};

const PACKAGES: &str = include_str!("../examples/openpnp_package.xml");
const PARTS: &str = include_str!("../examples/openpnp_parts.xml");
const MACHINE: &str = include_str!("../examples/openpnp_machine.xml");

fn imported() -> Imported {
    let mut i = Imported::default();
    i.read_packages(PACKAGES.as_bytes());
    i.read_parts(PARTS.as_bytes());
    i.read_machine(MACHINE.as_bytes());
    i.link();
    i
}

// ---- the XML reader

fn problem(doc: &str) -> Problem {
    parse(doc.as_bytes()).unwrap_err().problem
}

#[test]
fn the_reader_keeps_the_shape_and_drops_the_text_comments_and_declarations() {
    let e = parse(b"\xEF\xBB\xBF<?xml version='1.0'?>\n<!-- c --><a>\n text <![CDATA[<b/>]]>\n<b k='v &amp; w'/><c/></a>\n").unwrap();
    assert_eq!(e.name, "a");
    assert_eq!(e.children.len(), 2);
    assert_eq!(e.child("b").unwrap().attr("k"), Some("v & w"));
    assert_eq!(e.child("b").unwrap().line, 4);
}

#[test]
fn what_is_not_well_formed_is_refused_with_a_line() {
    for (doc, want) in [
        ("<a>", Problem::Unclosed("a".into())),
        ("<a></b>", Problem::Mismatch("a".into(), "b".into())),
        ("<a/><b/>", Problem::SeveralRoots),
        ("", Problem::Empty),
        ("<a x=1/>", Problem::BadMarkup),
        ("<a x=\"1\"", Problem::Unclosed("a".into())),
        ("<a x=\"&nope;\"/>", Problem::BadEntity),
        ("<a x=\"&#xFFFFFFFF;\"/>", Problem::BadEntity),
        ("<a x=\"<\"/>", Problem::BadMarkup),
        ("</a>", Problem::BadMarkup),
        ("<a><!-- never ends", Problem::BadMarkup),
    ] {
        assert_eq!(problem(doc), want, "{doc}");
    }
    let e = parse(b"<a>\n<b>\n</a>").unwrap_err();
    assert_eq!(e.line, 3);
}

#[test]
fn a_document_type_and_entities_are_refused_so_nothing_can_be_expanded() {
    let bomb = "<!DOCTYPE lolz [<!ENTITY lol \"lol\"><!ENTITY lol2 \"&lol;&lol;&lol;&lol;\">]><lolz>&lol2;</lolz>";
    assert_eq!(problem(bomb), Problem::Doctype);
    assert_eq!(problem("<!ENTITY x 'y'><a/>"), Problem::Doctype);
    // the five of XML and the numeric ones are read
    let e = parse(b"<a v=\"&lt;&gt;&amp;&quot;&apos;&#65;&#x42;\"/>").unwrap();
    assert_eq!(e.attr("v"), Some("<>&\"'AB"));
}

#[test]
fn the_limits_of_size_depth_elements_attributes_names_and_values_are_kept() {
    assert_eq!(
        parse(&vec![b' '; MAX_BYTES + 1]).unwrap_err().problem,
        Problem::TooBig
    );
    let deep = |n: usize| format!("{}{}", "<a>".repeat(n), "</a>".repeat(n));
    assert!(parse(deep(MAX_DEPTH).as_bytes()).is_ok());
    assert_eq!(problem(&deep(MAX_DEPTH + 1)), Problem::TooDeep);
    let wide = format!("<r>{}</r>", "<e/>".repeat(MAX_ELEMENTS));
    assert_eq!(problem(&wide), Problem::TooMany);
    let attrs = |n: usize| {
        format!(
            "<a {}/>",
            (0..n)
                .map(|i| format!("k{i}=\"1\""))
                .collect::<Vec<_>>()
                .join(" ")
        )
    };
    assert!(parse(attrs(MAX_ATTRIBUTES).as_bytes()).is_ok());
    assert_eq!(
        problem(&attrs(MAX_ATTRIBUTES + 1)),
        Problem::TooManyAttributes
    );
    assert_eq!(
        problem(&format!("<{}/>", "n".repeat(MAX_NAME + 1))),
        Problem::NameTooLong
    );
    assert_eq!(
        problem(&format!("<a v=\"{}\"/>", "v".repeat(MAX_VALUE + 1))),
        Problem::ValueTooLong
    );
    assert!(parse(format!("<a v=\"{}\"/>", "v".repeat(MAX_VALUE)).as_bytes()).is_ok());
    assert_eq!(
        parse(b"<a>\n\xFF\xFE</a>").unwrap_err().problem,
        Problem::NotText
    );
}

#[test]
fn the_message_of_every_problem_is_in_the_eight_languages() {
    let problems = [
        Problem::TooBig,
        Problem::NotText,
        Problem::Empty,
        Problem::Doctype,
        Problem::Unclosed("a".into()),
        Problem::Mismatch("a".into(), "b".into()),
        Problem::BadMarkup,
        Problem::TooDeep,
        Problem::TooMany,
        Problem::TooManyAttributes,
        Problem::NameTooLong,
        Problem::ValueTooLong,
        Problem::BadEntity,
        Problem::SeveralRoots,
    ];
    for p in problems {
        let e = eh3d_phoenix_pnp::xmlmini::XmlError {
            line: 7,
            problem: p,
        };
        let texts: std::collections::BTreeSet<String> =
            Lang::ALL.iter().map(|l| e.localized(*l)).collect();
        assert_eq!(texts.len(), 8, "{e:?}");
        assert!(texts.iter().all(|t| !t.contains('{')));
    }
}

// ---- the units

#[test]
fn the_units_come_to_millimetres() {
    for (name, f) in [
        ("Millimeters", 1.0),
        ("centimeters", 10.0),
        ("METERS", 1000.0),
        ("Inches", 25.4),
        ("Feet", 304.8),
        ("Mils", 0.0254),
        ("Microns", 0.001),
    ] {
        assert_eq!(unit_factor(name), Some(f), "{name}");
    }
    assert_eq!(unit_factor("furlongs"), None);
    assert_eq!(unit_factor(""), None);
}

// ---- the mapping

#[test]
fn the_example_files_give_the_tables_of_this_program() {
    let i = imported();
    assert_eq!(i.errors(), 0, "{:?}", i.findings);
    assert_eq!(i.packages.len(), 4);
    assert_eq!(i.parts.len(), 4);
    assert_eq!(
        i.feeders.len(),
        4,
        "the feeder that is switched off is left out"
    );
    // the body in the units of the file, in millimetres: long side first
    let qfn = i.packages.iter().find(|p| p.id == "QFN-32").unwrap();
    assert!((qfn.body_long_mm.unwrap() - 5.0).abs() < 1e-3);
    let led = i.packages.iter().find(|p| p.id == "LED_0603").unwrap();
    assert!(
        (led.body_long_mm.unwrap() - 1.6002).abs() < 1e-3
            && (led.body_short_mm.unwrap() - 0.8001).abs() < 1e-3
    );
    // a height in mils and a place in centimetres and in inches
    let led_part = i.parts.iter().find(|p| p.id == "LED").unwrap();
    assert!((led_part.height_mm.unwrap() - 0.5994).abs() < 1e-3);
    let f2 = i.feeders.iter().find(|f| f.id == "F2").unwrap();
    assert_eq!((f2.x, f2.y, f2.count), (-20.0, 20.0, 100));
    let f3 = i.feeders.iter().find(|f| f.id == "F3").unwrap();
    assert!((f3.x + 20.0).abs() < 1e-3 && (f3.y - 40.0).abs() < 1e-3 && f3.count == 10);
    assert_eq!(
        f3.package, "QFN-32",
        "a feeder takes its package from its part"
    );
}

#[test]
fn the_tables_that_are_written_are_read_by_the_program_itself() {
    let i = imported();
    let mut c = Catalog::new();
    c.add_packages(&i.packages_csv()).unwrap();
    c.add_parts(&i.parts_csv()).unwrap();
    assert!(c.package("QFN-32").is_some());
    assert!(c.package_body("QFN-32").is_some());
    let feeders = feeders_csv(&i.feeders_csv()).unwrap();
    assert_eq!(feeders.len(), 4);
    assert_eq!(feeders[0].id, "F1");
    // and the board of the examples is covered by them
    let parts = eh3d_phoenix_pnp::parts::parse(include_str!("../examples/board.pos")).unwrap();
    let a = eh3d_phoenix_pnp::feeders::assign(&parts, &feeders);
    assert!(a.without_feeder.is_empty(), "{:?}", a.without_feeder);
}

#[test]
fn what_is_not_mapped_is_counted_and_told() {
    let i = imported();
    let unmapped: Vec<_> = i
        .findings
        .iter()
        .filter_map(|f| match &f.issue {
            Issue::Unmapped(n, c) => Some((n.clone(), *c)),
            _ => None,
        })
        .collect();
    assert!(
        unmapped.contains(&("heads".to_string(), 1))
            && unmapped.contains(&("cameras".to_string(), 2)),
        "{unmapped:?}"
    );
    assert!(i
        .findings
        .iter()
        .any(|f| matches!(f.issue, Issue::Disabled(_)) && f.severity == Severity::Warning));
}

#[test]
fn a_row_that_cannot_be_used_is_an_error_with_its_line_and_the_others_are_kept() {
    let mut i = Imported::default();
    i.read_parts(
        b"<openpnp-parts>\n<part id=\"a\" package-id=\"P\" height=\"1\" height-units=\"Cubits\"/>\n<part id=\"b\" package-id=\"P\" height=\"x\"/>\n<part id=\"c\"/>\n<part package-id=\"P\"/>\n<part id=\"ok\" package-id=\"P\" height=\"0.4\"/>\n<part id=\"ok\" package-id=\"Q\"/>\n</openpnp-parts>",
    );
    assert_eq!(i.parts.len(), 1);
    assert_eq!(i.parts[0].id, "ok");
    let lines: Vec<usize> = i.findings.iter().map(|f| f.line).collect();
    assert_eq!(lines, vec![2, 3, 4, 5, 7]);
    assert_eq!(i.errors(), 4);
    assert!(matches!(i.findings[0].issue, Issue::UnknownUnit(_)));
    assert!(matches!(i.findings[1].issue, Issue::NotANumber(..)));
    assert!(matches!(i.findings[4].issue, Issue::DuplicateId(_)));
}

#[test]
fn a_file_that_is_not_the_one_asked_for_is_told_and_gives_nothing() {
    let mut i = Imported::default();
    i.read_packages(PARTS.as_bytes());
    assert!(i.packages.is_empty());
    assert!(
        matches!(&i.findings[0].issue, Issue::WrongRoot(a, b) if a == "openpnp-packages" && b == "openpnp-parts")
    );
    let mut i = Imported::default();
    i.read_machine(b"not xml at all");
    assert!(matches!(i.findings[0].issue, Issue::Xml(_)));
    assert!(i.feeders.is_empty());
}

#[test]
fn feeders_without_parts_or_with_unknown_parts_are_left_out_with_a_word() {
    let mut i = Imported::default();
    i.read_machine(MACHINE.as_bytes());
    i.link();
    assert!(i.feeders.is_empty());
    assert_eq!(
        i.findings
            .iter()
            .filter(|f| matches!(f.issue, Issue::NoPackageKnown(_)))
            .count(),
        4
    );
    let mut i = Imported::default();
    i.read_parts(b"<openpnp-parts><part id=\"10k\" package-id=\"R\"/></openpnp-parts>");
    i.read_machine(MACHINE.as_bytes());
    i.link();
    assert_eq!(i.feeders.len(), 1);
    assert_eq!(
        i.findings
            .iter()
            .filter(|f| matches!(f.issue, Issue::UnknownPart(..)))
            .count(),
        3
    );
}

#[test]
fn a_feeder_without_a_place_a_count_or_a_part_is_told() {
    let mut i = Imported::default();
    i.read_machine(
        b"<openpnp-machine>\n<feeders>\n<feeder part-id=\"a\"/>\n<feeder name=\"n\" part-id=\"a\"><location units=\"mm\" x=\"1\" y=\"2\"/></feeder>\n<feeder name=\"o\"><location x=\"1\" y=\"2\"/></feeder>\n<feeder name=\"p\" part-id=\"a\" count=\"-5\"><location x=\"1\" y=\"2\"/></feeder>\n</feeders>\n</openpnp-machine>",
    );
    assert_eq!(i.feeders.len(), 1);
    assert_eq!((i.feeders[0].id.as_str(), i.feeders[0].count), ("n", 0));
    let kinds: Vec<&Issue> = i.findings.iter().map(|f| &f.issue).collect();
    assert!(kinds.iter().any(|k| matches!(k, Issue::NoLocation(_))));
    assert!(kinds.iter().any(|k| matches!(k, Issue::NoCount(_))));
    assert!(kinds.iter().any(|k| matches!(k, Issue::Missing(..))));
    assert!(kinds.iter().any(|k| matches!(k, Issue::NotANumber(..))));
}

#[test]
fn every_finding_is_told_in_the_eight_languages_with_its_file_and_line() {
    let mut i = Imported::default();
    i.read_packages(PARTS.as_bytes());
    i.read_parts(b"<openpnp-parts><part id=\"a\" package-id=\"P\" height=\"x\"/><part id=\"d\" package-id=\"Z\"/><part id=\"d\" package-id=\"Z\"/><part/></openpnp-parts>");
    i.read_machine(MACHINE.as_bytes());
    i.read_machine(
        b"<openpnp-machine><feeders><feeder part-id=\"q\"/></feeders></openpnp-machine>",
    );
    i.link();
    i.read_machine(b"<broken");
    let mut kinds = std::collections::HashSet::new();
    for f in &i.findings {
        kinds.insert(std::mem::discriminant(&f.issue));
        let texts: std::collections::BTreeSet<String> =
            Lang::ALL.iter().map(|l| message(f, *l)).collect();
        assert!(texts.len() >= 7, "{f:?}: {texts:?}");
        assert!(
            texts.iter().all(|t| !t.contains('{') && t.contains(f.file)),
            "{f:?}"
        );
    }
    assert!(kinds.len() >= 7, "{}", kinds.len());
}

// ---- files made to hurt it

fn next(state: &mut u64) -> usize {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (*state >> 33) as usize
}

#[test]
fn the_example_files_cut_at_every_place_and_damaged_by_chance_never_panic() {
    for (text, kind) in [(PACKAGES, 0), (PARTS, 1), (MACHINE, 2)] {
        let bytes = text.as_bytes();
        let read = |b: &[u8]| {
            let mut i = Imported::default();
            match kind {
                0 => i.read_packages(b),
                1 => i.read_parts(b),
                _ => i.read_machine(b),
            }
            i.link();
            i
        };
        for cut in (0..bytes.len()).step_by(7) {
            let _ = read(&bytes[..cut]);
        }
        let mut s = 11u64 + kind as u64;
        for _ in 0..300 {
            let mut b = bytes.to_vec();
            for _ in 0..(1 + next(&mut s) % 6) {
                let at = next(&mut s) % b.len();
                match next(&mut s) % 4 {
                    0 => b[at] = (next(&mut s) % 256) as u8,
                    1 => {
                        b.remove(at);
                    }
                    2 => b.insert(at, b"<>&\"'/="[next(&mut s) % 7]),
                    _ => b.truncate(at.max(1)),
                }
            }
            let i = read(&b);
            // whatever was read is whole: a row has an id
            assert!(i.packages.iter().all(|p| !p.id.is_empty()));
            assert!(i
                .parts
                .iter()
                .all(|p| !p.id.is_empty() && !p.package.is_empty()));
            assert!(i.feeders.iter().all(|f| f.x.is_finite() && f.y.is_finite()));
        }
    }
}

#[test]
fn garbage_bytes_never_panic_the_reader() {
    let mut s = 5u64;
    for _ in 0..500 {
        let n = next(&mut s) % 200;
        let b: Vec<u8> = (0..n).map(|_| (next(&mut s) % 256) as u8).collect();
        let _ = parse(&b);
        let alphabet = b"<>/=\"' &;!?-[]abc\n";
        let t: Vec<u8> = (0..n)
            .map(|_| alphabet[next(&mut s) % alphabet.len()])
            .collect();
        let _ = parse(&t);
    }
}
