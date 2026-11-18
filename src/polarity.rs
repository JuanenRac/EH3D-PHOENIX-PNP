//! The sheet of the parts that have a direction. A diode, an LED, a chip with a mark for its first pin, a tantalum or an electrolytic capacitor, a connector or a transistor goes on the
//! board the right way round or it does not work; after a job, a person looks at each of them, and a sheet that lists them with their place and their turn makes that look short and
//! complete. Which parts have a direction is told by two lists of the configuration, `polarity_refs` (the start of the reference) and `polarity_packages` (a part of the package), both
//! with `*` and `?` and without regard to case: a part is on the sheet when either of them matches.

use crate::parts::{csv_field, Part, Side};
use crate::rotation::glob_match;

/// Whether a part has a direction under these lists.
pub fn is_polarized(part: &Part, refs: &[String], packages: &[String]) -> bool {
    refs.iter().any(|p| glob_match(p, &part.reference))
        || packages.iter().any(|p| glob_match(p, &part.package))
}

/// The parts that have a direction, the top side before the bottom side and each side in the order of the numbers inside the references (`D2` before `D10`).
pub fn sheet(parts: &[Part], refs: &[String], packages: &[String]) -> Vec<Part> {
    let mut out: Vec<Part> = parts
        .iter()
        .filter(|p| is_polarized(p, refs, packages))
        .cloned()
        .collect();
    out.sort_by(|a, b| {
        let side = |p: &Part| u8::from(p.side == Side::Bottom);
        side(a)
            .cmp(&side(b))
            .then_with(|| natural_key(&a.reference).cmp(&natural_key(&b.reference)))
    });
    out
}

/// The letters of a reference and then its numbers, so that `R2` comes before `R10`.
fn natural_key(reference: &str) -> (String, Vec<u64>, String) {
    let letters: String = reference
        .chars()
        .take_while(|c| !c.is_ascii_digit())
        .collect::<String>()
        .to_lowercase();
    let numbers = reference
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<u64>().unwrap_or(u64::MAX))
        .collect();
    (letters, numbers, reference.to_lowercase())
}

/// The sheet as a CSV: the columns `reference,value,package,side,x_mm,y_mm,rotation_deg,checked`; the last one is empty, for the person who looks to fill in.
pub fn to_csv(rows: &[Part]) -> String {
    let mut out = String::from("reference,value,package,side,x_mm,y_mm,rotation_deg,checked\n");
    for p in rows {
        out.push_str(&format!(
            "{},{},{},{},{:.3},{:.3},{:.1},\n",
            csv_field(&p.reference),
            csv_field(&p.value),
            csv_field(&p.package),
            if p.side == Side::Bottom {
                "bottom"
            } else {
                "top"
            },
            p.x,
            p.y,
            p.rotation
        ));
    }
    out
}

/// How many parts of the sheet are on each side, as a line of text.
pub fn summary(rows: &[Part], total: usize) -> String {
    let bottom = rows.iter().filter(|p| p.side == Side::Bottom).count();
    format!(
        "{} of {total} parts have a direction ({} on the top side, {bottom} on the bottom side)",
        rows.len(),
        rows.len() - bottom
    )
}
