// EH3D-PHOENIX-PNP - the space between the bodies of the parts of a board.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A nozzle puts a part down and lets go; a part that is placed with its body nearer to the next than a nozzle tip and the tolerance of the placement allow either pushes it or is pushed. The
//! table of packages gives the size of the body of each package; with the places and the turns of the parts this finds the pairs on the same side of the board whose bodies are nearer than
//! a minimum (or lie on one another, which a position file with a wrong package or a wrong origin gives). Each body is taken as the rectangle that holds it once turned, so a part turned by
//! 45 degrees is judged by the box around it, which is a little cautious. It reads the table of parts and the table of packages and changes nothing.

use crate::catalog::Catalog;
use crate::parts::{csv_field, Part};

/// The most pairs that are reported (the nearest first).
pub const MAX_PAIRS: usize = 1000;

/// Two parts that are nearer than the minimum.
#[derive(Debug, Clone, PartialEq)]
pub struct Pair {
    pub first: String,
    pub second: String,
    /// The space between the two boxes in millimetres; negative when they overlap (how far one would have to move).
    pub gap_mm: f64,
}

/// What the check found.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Clearance {
    pub pairs: Vec<Pair>,
    /// The parts whose body has no size in the tables, which are left out.
    pub no_size: Vec<String>,
    /// The parts that were compared.
    pub checked: usize,
    /// Whether there were more pairs than `MAX_PAIRS`.
    pub truncated: bool,
}

struct Body<'a> {
    reference: &'a str,
    side: crate::parts::Side,
    x: f64,
    y: f64,
    hx: f64,
    hy: f64,
}

/// The half sides of the box around a body of `long` by `short` turned by `degrees`.
pub fn half_box(long: f64, short: f64, degrees: f64) -> (f64, f64) {
    let (s, c) = degrees.to_radians().sin_cos();
    (
        (long * c.abs() + short * s.abs()) / 2.0,
        (long * s.abs() + short * c.abs()) / 2.0,
    )
}

/// The space between two boxes: the distance between the nearest points when they are apart, or minus the smallest move that separates them when they overlap.
pub fn gap(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> f64 {
    let gx = (a.0 - b.0).abs() - (a.2 + b.2);
    let gy = (a.1 - b.1).abs() - (a.3 + b.3);
    if gx > 0.0 && gy > 0.0 {
        gx.hypot(gy)
    } else {
        gx.max(gy)
    }
}

/// Finds the pairs of parts whose bodies are less than `min_mm` apart.
pub fn check(parts: &[Part], catalog: &Catalog, min_mm: f64) -> Clearance {
    let mut found = Clearance::default();
    let mut bodies: Vec<Body> = Vec::new();
    for p in parts {
        match catalog.body_size(p) {
            Some((long, short)) => {
                let (hx, hy) = half_box(long, short, p.rotation);
                bodies.push(Body {
                    reference: &p.reference,
                    side: p.side,
                    x: p.x,
                    y: p.y,
                    hx,
                    hy,
                });
            }
            None => found.no_size.push(p.reference.clone()),
        }
    }
    found.no_size.sort();
    found.checked = bodies.len();
    bodies.sort_by(|a, b| {
        (a.x - a.hx)
            .total_cmp(&(b.x - b.hx))
            .then_with(|| a.reference.cmp(b.reference))
    });
    let mut pairs: Vec<Pair> = Vec::new();
    for (i, a) in bodies.iter().enumerate() {
        for b in &bodies[i + 1..] {
            // the boxes are sorted by their left edge: from here on none can be within reach
            if b.x - b.hx - (a.x + a.hx) >= min_mm {
                break;
            }
            if a.side != b.side {
                continue;
            }
            let g = gap((a.x, a.y, a.hx, a.hy), (b.x, b.y, b.hx, b.hy));
            if g < min_mm {
                let (first, second) = if a.reference <= b.reference {
                    (a.reference, b.reference)
                } else {
                    (b.reference, a.reference)
                };
                pairs.push(Pair {
                    first: first.to_string(),
                    second: second.to_string(),
                    gap_mm: g,
                });
            }
        }
    }
    pairs.sort_by(|a, b| {
        a.gap_mm
            .total_cmp(&b.gap_mm)
            .then_with(|| a.first.cmp(&b.first))
            .then_with(|| a.second.cmp(&b.second))
    });
    found.truncated = pairs.len() > MAX_PAIRS;
    pairs.truncate(MAX_PAIRS);
    found.pairs = pairs;
    found
}

impl Clearance {
    /// Whether any pair is nearer than the minimum.
    pub fn is_clear(&self) -> bool {
        self.pairs.is_empty()
    }

    /// The pairs whose boxes overlap.
    pub fn overlaps(&self) -> usize {
        self.pairs.iter().filter(|p| p.gap_mm < 0.0).count()
    }

    /// The report for a person.
    pub fn text(&self, min_mm: f64) -> String {
        let mut out = String::new();
        for p in &self.pairs {
            if p.gap_mm < 0.0 {
                out.push_str(&format!(
                    "{} and {} overlap by {:.3} mm\n",
                    p.first, p.second, -p.gap_mm
                ));
            } else {
                out.push_str(&format!(
                    "{} and {} are {:.3} mm apart\n",
                    p.first, p.second, p.gap_mm
                ));
            }
        }
        if self.truncated {
            out.push_str(&format!("only the nearest {MAX_PAIRS} pairs are listed\n"));
        }
        if !self.no_size.is_empty() {
            out.push_str(&format!("no body size for: {}\n", self.no_size.join(" ")));
        }
        out.push_str(&format!(
            "{} parts compared: {} pairs nearer than {min_mm} mm, {} of them overlapping\n",
            self.checked,
            self.pairs.len(),
            self.overlaps()
        ));
        out
    }

    /// The pairs as a CSV: `first,second,gap_mm`.
    pub fn csv(&self) -> String {
        let mut out = String::from("first,second,gap_mm\n");
        for p in &self.pairs {
            out.push_str(&format!(
                "{},{},{:.4}\n",
                csv_field(&p.first),
                csv_field(&p.second),
                p.gap_mm
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parts::Side;

    fn part(r: &str, x: f64, y: f64, rot: f64, side: Side) -> Part {
        Part {
            reference: r.into(),
            value: "1k".into(),
            package: "0603".into(),
            x,
            y,
            rotation: rot,
            side,
        }
    }

    fn catalog() -> Catalog {
        let mut c = Catalog::new();
        // a body 2 mm by 1 mm
        c.add_packages("package,body_long_mm,body_short_mm\n0603,2,1\n")
            .unwrap();
        c
    }

    #[test]
    fn the_box_of_a_turned_body_is_the_box_around_it() {
        let (hx, hy) = half_box(2.0, 1.0, 0.0);
        assert!((hx - 1.0).abs() < 1e-9 && (hy - 0.5).abs() < 1e-9);
        let (hx, hy) = half_box(2.0, 1.0, 90.0);
        assert!((hx - 0.5).abs() < 1e-9 && (hy - 1.0).abs() < 1e-9);
        let (hx, hy) = half_box(2.0, 1.0, 45.0);
        assert!((hx - hy).abs() < 1e-9);
        assert!((hx - 1.5 * std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-9);
        let (hx, _) = half_box(2.0, 1.0, 180.0);
        assert!((hx - 1.0).abs() < 1e-9);
    }

    #[test]
    fn the_gap_is_the_distance_apart_or_minus_the_overlap() {
        let a = (0.0, 0.0, 1.0, 0.5);
        assert!((gap(a, (3.0, 0.0, 1.0, 0.5)) - 1.0).abs() < 1e-9);
        assert!((gap(a, (3.0, 1.5, 1.0, 0.5)) - 1.0f64.hypot(0.5)).abs() < 1e-9);
        assert!((gap(a, (1.5, 0.0, 1.0, 0.5)) + 0.5).abs() < 1e-9);
        assert!((gap(a, a) + 1.0).abs() < 1e-9);
        assert_eq!(gap(a, (2.0, 0.0, 1.0, 0.5)), 0.0);
    }

    #[test]
    fn parts_too_near_are_found_nearest_first() {
        let parts = vec![
            part("R1", 0.0, 0.0, 0.0, Side::Top),
            part("R2", 2.1, 0.0, 0.0, Side::Top),
            part("R3", 10.0, 0.0, 0.0, Side::Top),
            part("R4", 10.0, 0.8, 0.0, Side::Top),
        ];
        let c = check(&parts, &catalog(), 0.2);
        assert_eq!(c.checked, 4);
        assert_eq!(c.pairs.len(), 2);
        // R3 and R4 overlap by 0.2 (boxes 1 mm tall, 0.8 apart), R1 and R2 are 0.1 apart
        assert_eq!(
            (c.pairs[0].first.as_str(), c.pairs[0].second.as_str()),
            ("R3", "R4")
        );
        assert!((c.pairs[0].gap_mm + 0.2).abs() < 1e-9);
        assert!((c.pairs[1].gap_mm - 0.1).abs() < 1e-9);
        assert_eq!(c.overlaps(), 1);
        assert!(!c.is_clear());
    }

    #[test]
    fn a_turn_changes_who_is_near() {
        let parts = vec![
            part("R1", 0.0, 0.0, 0.0, Side::Top),
            part("R2", 1.6, 0.0, 90.0, Side::Top),
        ];
        // turned, R2 is 1 mm wide: the gap is 1.6 - 1 - 0.5 = 0.1
        let c = check(&parts, &catalog(), 0.2);
        assert_eq!(c.pairs.len(), 1);
        assert!((c.pairs[0].gap_mm - 0.1).abs() < 1e-9);
        let c = check(&parts, &catalog(), 0.05);
        assert!(c.is_clear());
    }

    #[test]
    fn parts_on_the_two_sides_do_not_collide() {
        let parts = vec![
            part("R1", 0.0, 0.0, 0.0, Side::Top),
            part("R2", 0.0, 0.0, 0.0, Side::Bottom),
        ];
        assert!(check(&parts, &catalog(), 0.2).is_clear());
    }

    #[test]
    fn a_part_without_a_body_size_is_left_out_and_named() {
        let mut p = part("X1", 0.0, 0.0, 0.0, Side::Top);
        p.package = "Mystery".into();
        let parts = vec![part("R1", 0.0, 0.0, 0.0, Side::Top), p];
        let c = check(&parts, &catalog(), 0.2);
        assert_eq!(c.checked, 1);
        assert_eq!(c.no_size, vec!["X1".to_string()]);
        assert!(c.text(0.2).contains("no body size for: X1"));
    }

    #[test]
    fn the_sweep_finds_the_same_pairs_as_looking_at_all_of_them() {
        // a board made from a fixed seed
        let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % 10_000) as f64 / 1000.0
        };
        for round in 0..12 {
            let parts: Vec<Part> = (0..60)
                .map(|i| {
                    let rot = [0.0, 90.0, 45.0, 30.0][(i + round) % 4];
                    let side = if i % 5 == 0 { Side::Bottom } else { Side::Top };
                    part(&format!("R{i:02}"), next(), next(), rot, side)
                })
                .collect();
            let min = 0.15;
            let found = check(&parts, &catalog(), min);
            let mut brute = 0usize;
            for (i, a) in parts.iter().enumerate() {
                for b in &parts[i + 1..] {
                    if a.side != b.side {
                        continue;
                    }
                    let (ahx, ahy) = half_box(2.0, 1.0, a.rotation);
                    let (bhx, bhy) = half_box(2.0, 1.0, b.rotation);
                    if gap((a.x, a.y, ahx, ahy), (b.x, b.y, bhx, bhy)) < min {
                        brute += 1;
                    }
                }
            }
            assert_eq!(
                found.pairs.len().min(MAX_PAIRS),
                brute.min(MAX_PAIRS),
                "round {round}"
            );
        }
    }

    #[test]
    fn the_text_and_the_table_name_the_pairs() {
        let parts = vec![
            part("R2", 0.5, 0.0, 0.0, Side::Top),
            part("R1", 0.0, 0.0, 0.0, Side::Top),
        ];
        let c = check(&parts, &catalog(), 0.2);
        let text = c.text(0.2);
        assert!(text.contains("R1 and R2 overlap by 1.000 mm"));
        assert!(
            text.ends_with("2 parts compared: 1 pairs nearer than 0.2 mm, 1 of them overlapping\n")
        );
        assert_eq!(c.csv(), "first,second,gap_mm\nR1,R2,-1.0000\n");
    }
}
