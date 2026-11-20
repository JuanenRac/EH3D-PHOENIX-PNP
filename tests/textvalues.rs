//! A value that looks like a number is text: a part number such as `0402` or `007`, a value such as `1e3`, a reference such as `1234` keep their zeros, their letters and their spelling through
//! every reader of a table and every writer of one. These tests run each table with such values and fail if a reader turns one of them into a number.

use eh3d_phoenix_pnp::asmexport::{self, Format, Sides, Unit};
use eh3d_phoenix_pnp::bom::Bom;
use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::feeders::from_csv as feeders_csv;
use eh3d_phoenix_pnp::lottrace::parse_lots;
use eh3d_phoenix_pnp::parts::{parse, Side};

const LOOKS_LIKE_NUMBERS: [&str; 6] = ["0402", "007", "1e3", "0.10", "00100", "+5"];

#[test]
fn a_table_of_parts_keeps_the_value_the_package_and_the_reference_as_written() {
    for v in LOOKS_LIKE_NUMBERS {
        let csv = format!("ref,val,package,x,y,rot,side\n{v},{v},{v},10,20,0,top\n");
        let parts = parse(&csv).unwrap();
        assert_eq!(parts[0].reference, v);
        assert_eq!(parts[0].value, v);
        assert_eq!(parts[0].package, v);
        assert_eq!(parts[0].side, Side::Top);
    }
}

#[test]
fn what_is_written_for_assemblers_keeps_them_too() {
    let csv = "ref,val,package,x,y,rot,side\n0001,00100,0402,10,20,90,top\n";
    let parts = parse(csv).unwrap();
    let out = asmexport::write(&parts, Format::Cpl, Unit::Mm, Sides::All);
    assert!(
        out.contains("0001") && out.contains("00100") && out.contains("0402"),
        "{out}"
    );
    let back = eh3d_phoenix_pnp::asmimport::parse(&out).unwrap();
    assert_eq!(back.parts[0].reference, "0001");
    assert_eq!(back.parts[0].value, "00100");
    assert_eq!(back.parts[0].package, "0402");
}

#[test]
fn a_table_of_feeders_keeps_the_id_the_value_and_the_package() {
    for v in LOOKS_LIKE_NUMBERS {
        let feeders = feeders_csv(&format!(
            "id,value,package,x,y,count\n{v},{v},{v},-20,10,100\n"
        ))
        .unwrap();
        assert_eq!(
            (
                feeders[0].id.as_str(),
                feeders[0].value.as_str(),
                feeders[0].package.as_str()
            ),
            (v, v, v)
        );
    }
}

#[test]
fn the_tables_of_packages_and_parts_keep_the_names() {
    let mut c = Catalog::new();
    c.add_packages("package,height_mm\n0402,0.35\n0603,0.5\n")
        .unwrap();
    assert!(c.package("0402").is_some());
    assert!(c.package_names().contains(&"0603".to_string()));
    c.add_parts("value,package,height_mm,rank\n00100,0402,,\n")
        .unwrap();
    let csv = c.packages_csv();
    assert!(csv.contains("0402") && csv.contains("0603"), "{csv}");
}

#[test]
fn a_bill_of_materials_and_a_table_of_lots_keep_the_part_numbers() {
    let bom = Bom::parse(
        "Designator,Comment,Footprint,Quantity,Part Number\nR1,00100,0402,1,0001234\n",
        100,
    )
    .unwrap();
    assert_eq!(bom.lines[0].value, "00100");
    assert_eq!(bom.lines[0].package, "0402");
    assert_eq!(bom.lines[0].part_number, "0001234");
    let lots = parse_lots("feeder,lot,reel,date_code,supplier\nF1,00123,0007,2105,0099\n").unwrap();
    assert_eq!(
        (
            lots[0].lot.as_str(),
            lots[0].reel.as_str(),
            lots[0].date_code.as_str(),
            lots[0].supplier.as_str()
        ),
        ("00123", "0007", "2105", "0099")
    );
}
