//! A value that looks like a number is text: a part number such as 1234, a package such as 0402 and an identifier with leading zeros such as 007 are kept as they were written by every reader of
//! a table, and by the writers that give a table back. Another program lost the leading zeros of a package when it read a table; this is the test that keeps this one from doing so.

use eh3d_phoenix_pnp::bom::Bom;
use eh3d_phoenix_pnp::feeders;
use eh3d_phoenix_pnp::feeding;
use eh3d_phoenix_pnp::loadsheet::slots_from_csv;
use eh3d_phoenix_pnp::parts::{from_csv, parse};
use eh3d_phoenix_pnp::slotbank::{Geometry, Layout};

const LOOKS_NUMERIC: [&str; 6] = ["0402", "007", "1234", "1e3", "00.50", "+12"];

#[test]
fn the_values_and_packages_of_a_table_of_parts_keep_what_was_written() {
    for v in LOOKS_NUMERIC {
        let text = format!("ref,val,package,x,y,rot,side\nR1,{v},{v},1,2,0,top\n");
        let parts = from_csv(&text).unwrap();
        assert_eq!(
            (parts[0].value.as_str(), parts[0].package.as_str()),
            (v, v),
            "{v}"
        );
        // a position file with the columns in the order of a KiCad file
        let pos = format!("# Ref Val Package PosX PosY Rot Side\nR1 {v} {v} 1.0 2.0 0.0 top\n");
        let parts = parse(&pos).unwrap();
        assert_eq!(
            (parts[0].value.as_str(), parts[0].package.as_str()),
            (v, v),
            "{v}"
        );
    }
    // a reference that is only digits is kept too, with its zeros
    let parts = from_csv("ref,val,package,x,y,rot,side\n0007,1k,0402,1,2,0,top\n").unwrap();
    assert_eq!(parts[0].reference, "0007");
}

#[test]
fn the_identifiers_values_and_packages_of_the_tables_of_feeders_keep_what_was_written() {
    for v in LOOKS_NUMERIC {
        let basic = feeders::from_csv(&format!("id,value,package,x,y,count\n{v},{v},{v},1,2,10\n"))
            .unwrap();
        assert_eq!(
            (
                basic[0].id.as_str(),
                basic[0].value.as_str(),
                basic[0].package.as_str()
            ),
            (v, v, v),
            "{v}"
        );
        let full = feeding::from_csv(&format!(
            "id,value,package,x,y,count,priority,kind,cols,rows,pitch_x,pitch_y,pitch,rotation,enabled,warn_below,slot,fill\n{v},{v},{v},1,2,10,normal,tape,,,,,,0,yes,0,{v},\n"
        ))
        .unwrap();
        let b = &full[0].base;
        assert_eq!(
            (b.id.as_str(), b.value.as_str(), b.package.as_str()),
            (v, v, v),
            "{v}"
        );
        assert_eq!(full[0].slot.as_deref(), Some(v), "{v}");
    }
}

#[test]
fn the_part_numbers_of_a_bill_of_materials_and_the_names_of_the_slots_keep_what_was_written() {
    for v in LOOKS_NUMERIC {
        let bom = Bom::parse(
            &format!("references,value,package,quantity,mpn\nR1,{v},{v},1,{v}\n"),
            1000,
        )
        .unwrap();
        let l = &bom.lines[0];
        assert_eq!(
            (l.value.as_str(), l.package.as_str(), l.part_number.as_str()),
            (v, v, v),
            "{v}"
        );
        let slots = slots_from_csv(&format!("id,x,y,max_width_mm\n{v},1,2,8\n")).unwrap();
        assert_eq!(slots[0].id, v, "{v}");
    }
}

#[test]
fn the_table_of_the_banks_gives_the_names_back_as_it_took_them() {
    let geo = Geometry {
        pitch_mm: 4.0,
        gap_mm: 4.0,
        slots: 60,
        reach_mm: 240.0,
    };
    let text = "id,bank,first_slot,tape_mm,state,chain\n007,left,1,8,,\n0402,right,1,8,,\n1e3,left,4,8,,\n";
    let l = Layout::from_csv(text, geo).unwrap();
    let out = l.to_csv();
    for id in ["007", "0402", "1e3"] {
        assert!(out.contains(&format!("\n{id},")), "{id}: {out}");
    }
    assert_eq!(Layout::from_csv(&out, geo).unwrap().to_csv(), out);
}

#[test]
fn the_catalog_the_prices_and_the_lots_keep_what_was_written() {
    use eh3d_phoenix_pnp::bomcost::parse_prices;
    use eh3d_phoenix_pnp::catalog::Catalog;
    use eh3d_phoenix_pnp::lottrace::parse_lots;
    for v in LOOKS_NUMERIC {
        // the name of a package is shown as it was written, in the table that is given back as well
        let mut catalog = Catalog::new();
        catalog
            .add_packages(&format!("package,nozzle_tip\n{v},N45\n"))
            .unwrap();
        assert_eq!(catalog.package_names(), [v.to_string()], "{v}");
        assert!(
            catalog.packages_csv().contains(&format!("\n{v},")),
            "{v}: {}",
            catalog.packages_csv()
        );
        // a part number, and the package of a price
        let prices = parse_prices(&format!(
            "part_number,value,package,qty_from,price\n{v},,{v},1,0.10\n"
        ))
        .unwrap();
        assert_eq!(
            (prices[0].part_number.as_str(), prices[0].package.as_str()),
            (v, v),
            "{v}"
        );
        // the lot, the reel and the date code of a reel
        let lots = parse_lots(&format!(
            "feeder,lot,reel,date_code,supplier\n{v},{v},{v},{v},{v}\n"
        ))
        .unwrap();
        let l = &lots[0];
        assert_eq!(
            (
                l.feeder.as_str(),
                l.lot.as_str(),
                l.reel.as_str(),
                l.date_code.as_str()
            ),
            (v, v, v, v),
            "{v}"
        );
    }
}
