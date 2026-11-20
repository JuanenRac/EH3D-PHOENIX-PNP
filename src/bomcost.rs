// EH3D-PHOENIX-PNP - what the bill of materials of a run costs, from a table of prices with price breaks.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A supplier sells a part cheaper by the hundred than by the piece, and not at all below a minimum. A table of prices `part_number|value[,package],price[,qty_from]` gives, for a part, the
//! price of one piece from a quantity on; this takes the bill of materials of a board, the number of boards and a share for spares, works out how many pieces of each part are needed,
//! takes the price of the break that this quantity falls in (the quantity of the lowest break is bought when fewer are needed, which the report says), and adds it all up: the cost of the
//! run, of a board, and the parts that have no price. The currency is whatever the prices are in; nothing is converted.

use crate::bom::{normalize_value, Bom};
use crate::error::{Code, PnpError};
use crate::parts::{csv_field, detect_delimiter, read_records};

/// The most rows of a table of prices.
pub const MAX_PRICES: usize = 50_000;
/// The most boards.
pub const MAX_BOARDS: u64 = 1_000_000;

fn bad(detail: impl Into<String>) -> PnpError {
    PnpError::new(Code::BomBad, detail.into())
}

/// The price of one piece from a quantity on.
#[derive(Debug, Clone, PartialEq)]
pub struct Price {
    pub part_number: String,
    /// The value as `normalize_value` writes it, in lower case.
    pub value: String,
    pub package: String,
    pub from: u64,
    pub each: f64,
}

/// Reads the prices: a header with `price` and `part_number` or `value` (and, if wanted, `package` and `qty_from`).
pub fn parse_prices(text: &str) -> Result<Vec<Price>, PnpError> {
    let text = text.trim_start_matches('\u{feff}');
    let head_line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let records = read_records(text, detect_delimiter(head_line)).map_err(bad)?;
    let Some((header, rows)) = records.split_first() else {
        return Err(bad("the table of prices is empty"));
    };
    let header: Vec<String> = header
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();
    let column = |names: &[&str]| header.iter().position(|h| names.contains(&h.as_str()));
    let price_at = column(&["price", "unit_price"])
        .ok_or_else(|| bad("the table of prices needs the column price"))?;
    let (pn_at, value_at) = (
        column(&["part_number", "part number", "mpn"]),
        column(&["value"]),
    );
    if pn_at.is_none() && value_at.is_none() {
        return Err(bad(
            "the table of prices needs the column part_number or value",
        ));
    }
    let (package_at, from_at) = (column(&["package"]), column(&["qty_from", "min_qty"]));
    if rows.len() > MAX_PRICES {
        return Err(bad(format!(
            "more than {MAX_PRICES} rows in the table of prices"
        )));
    }
    let mut out: Vec<Price> = Vec::with_capacity(rows.len());
    for (n, row) in rows.iter().enumerate() {
        let line = n + 2;
        let get = |at: Option<usize>| at.and_then(|i| row.get(i)).map(|s| s.trim()).unwrap_or("");
        let each: f64 = get(Some(price_at))
            .replace(',', ".")
            .parse()
            .ok()
            .filter(|v: &f64| v.is_finite() && *v >= 0.0 && *v <= 1.0e6)
            .ok_or_else(|| {
                bad(format!(
                    "row {line}: the price is a number from 0 to 1000000"
                ))
            })?;
        let from = match get(from_at) {
            "" => 1,
            v => v
                .parse::<u64>()
                .ok()
                .filter(|q| (1..=1_000_000_000).contains(q))
                .ok_or_else(|| bad(format!("row {line}: qty_from is a whole number from 1")))?,
        };
        let price = Price {
            part_number: get(pn_at).to_string(),
            value: normalize_value(get(value_at)).to_ascii_lowercase(),
            package: get(package_at).to_ascii_lowercase(),
            from,
            each,
        };
        if price.part_number.is_empty() && price.value.is_empty() {
            return Err(bad(format!(
                "row {line}: a part number or a value is needed"
            )));
        }
        if out.iter().any(|p| {
            p.from == price.from
                && p.part_number.eq_ignore_ascii_case(&price.part_number)
                && p.value == price.value
                && p.package == price.package
        }) {
            return Err(bad(format!(
                "row {line}: the same part has two prices for the same break"
            )));
        }
        out.push(price);
    }
    Ok(out)
}

/// One part to buy.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub value: String,
    pub package: String,
    pub part_number: String,
    pub per_board: u64,
    /// The pieces needed for the run with the spares.
    pub needed: u64,
    /// The pieces bought: the needed ones, or the lowest break when that is more.
    pub bought: u64,
    pub each: f64,
    pub cost: f64,
}

/// The cost of a run.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cost {
    pub boards: u64,
    pub lines: Vec<Line>,
    /// The parts that have no price, as `value package`.
    pub unpriced: Vec<String>,
    pub total: f64,
}

fn matching<'a>(prices: &'a [Price], pn: &str, value: &str, package: &str) -> Vec<&'a Price> {
    let by_pn: Vec<&Price> = if pn.is_empty() {
        Vec::new()
    } else {
        prices
            .iter()
            .filter(|p| p.part_number.eq_ignore_ascii_case(pn))
            .collect()
    };
    if !by_pn.is_empty() {
        return by_pn;
    }
    prices
        .iter()
        .filter(|p| {
            p.part_number.is_empty()
                && !p.value.is_empty()
                && p.value == value
                && (p.package.is_empty() || p.package == package)
        })
        .collect()
}

/// Costs the bill for `boards` boards with `spare_percent` per cent more pieces (rounded up).
pub fn cost(
    bom: &Bom,
    prices: &[Price],
    boards: u64,
    spare_percent: f64,
) -> Result<Cost, PnpError> {
    if !(1..=MAX_BOARDS).contains(&boards) {
        return Err(bad(format!("the boards are from 1 to {MAX_BOARDS}")));
    }
    if !(0.0..=1000.0).contains(&spare_percent) {
        return Err(bad("the spares are from 0 to 1000 per cent"));
    }
    let mut out = Cost {
        boards,
        ..Cost::default()
    };
    for l in &bom.lines {
        let per_board = l.references.len().max(l.quantity.unwrap_or(0)) as u64;
        if per_board == 0 {
            continue;
        }
        let needed =
            ((per_board * boards) as f64 * (1.0 + spare_percent / 100.0) - 1e-9).ceil() as u64;
        let value = normalize_value(&l.value).to_ascii_lowercase();
        let package = l.package.trim().to_ascii_lowercase();
        let mut options = matching(prices, l.part_number.trim(), &value, &package);
        if options.is_empty() {
            out.unpriced.push(
                format!("{} {}", l.value.trim(), l.package.trim())
                    .trim()
                    .to_string(),
            );
            continue;
        }
        options.sort_by_key(|p| p.from);
        let lowest = options[0].from;
        let bought = needed.max(lowest);
        let each = options
            .iter()
            .rfind(|p| p.from <= bought)
            .map_or(options[0].each, |p| p.each);
        out.total += each * bought as f64;
        out.lines.push(Line {
            value: l.value.trim().to_string(),
            package: l.package.trim().to_string(),
            part_number: l.part_number.trim().to_string(),
            per_board,
            needed,
            bought,
            each,
            cost: each * bought as f64,
        });
    }
    out.lines.sort_by(|a, b| {
        b.cost
            .total_cmp(&a.cost)
            .then_with(|| a.value.cmp(&b.value))
            .then_with(|| a.package.cmp(&b.package))
    });
    out.unpriced.sort();
    out.unpriced.dedup();
    Ok(out)
}

impl Cost {
    /// The cost of one board of the run.
    pub fn per_board(&self) -> f64 {
        self.total / self.boards.max(1) as f64
    }

    /// The report for a person; `currency` is a label (it may be empty).
    pub fn text(&self, currency: &str) -> String {
        let unit = if currency.is_empty() {
            String::new()
        } else {
            format!(" {currency}")
        };
        let mut out = String::new();
        for l in &self.lines {
            out.push_str(&format!(
                "{} {}{}: {} needed, {} bought at {:.4}{unit} = {:.2}{unit}{}\n",
                l.value,
                l.package,
                if l.part_number.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", l.part_number)
                },
                l.needed,
                l.bought,
                l.each,
                l.cost,
                if l.bought > l.needed {
                    format!(
                        " ({} more than needed: the lowest quantity sold)",
                        l.bought - l.needed
                    )
                } else {
                    String::new()
                }
            ));
        }
        if !self.unpriced.is_empty() {
            out.push_str(&format!("no price for: {}\n", self.unpriced.join(", ")));
        }
        out.push_str(&format!(
            "{} boards: {:.2}{unit} in all, {:.4}{unit} for a board\n",
            self.boards,
            self.total,
            self.per_board()
        ));
        out
    }

    /// The lines as a CSV: `value,package,part_number,per_board,needed,bought,each,cost`.
    pub fn csv(&self) -> String {
        let mut out = String::from("value,package,part_number,per_board,needed,bought,each,cost\n");
        for l in &self.lines {
            out.push_str(&format!(
                "{},{},{},{},{},{},{:.6},{:.4}\n",
                csv_field(&l.value),
                csv_field(&l.package),
                csv_field(&l.part_number),
                l.per_board,
                l.needed,
                l.bought,
                l.each,
                l.cost
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bom() -> Bom {
        Bom::parse(
            "reference,value,package,mpn\n\"R1,R2,R3\",10k,0603,RC0603-10K\nC1,100n,0402,\nU1,MCU,TQFP-48,STM32\n",
            100,
        )
        .unwrap()
    }

    const PRICES: &str = "part_number,value,package,qty_from,price\nRC0603-10K,,,1,0.10\nRC0603-10K,,,100,0.02\n,100n,0402,1,0.05\nSTM32,,,10,3.5\n";

    #[test]
    fn the_prices_are_read() {
        let p = parse_prices(PRICES).unwrap();
        assert_eq!(p.len(), 4);
        assert_eq!((p[1].from, p[1].each), (100, 0.02));
        assert_eq!(p[2].value, "100n");
        assert_eq!(parse_prices("value;price\n10k;0,5\n").unwrap()[0].each, 0.5);
    }

    #[test]
    fn prices_that_are_wrong_are_refused() {
        for text in [
            "",
            "part_number\nX\n",
            "price\n1\n",
            "part_number,price\nX,-1\n",
            "part_number,price\nX,abc\n",
            "part_number,price\nX,inf\n",
            "part_number,price,qty_from\nX,1,0\n",
            "part_number,price,qty_from\nX,1,two\n",
            "part_number,value,price\n,,1\n",
            "part_number,price\nX,1\nx,2\n",
        ] {
            assert!(parse_prices(text).is_err(), "{text:?}");
        }
    }

    #[test]
    fn the_price_of_the_break_that_the_quantity_falls_in_is_taken() {
        let p = parse_prices(PRICES).unwrap();
        // 3 per board: 10 boards = 30 pieces at 0.10; 40 boards = 120 pieces at 0.02
        let few = cost(&bom(), &p, 10, 0.0).unwrap();
        let r = few
            .lines
            .iter()
            .find(|l| l.part_number == "RC0603-10K")
            .unwrap();
        assert_eq!((r.needed, r.bought, r.each), (30, 30, 0.10));
        let many = cost(&bom(), &p, 40, 0.0).unwrap();
        let r = many
            .lines
            .iter()
            .find(|l| l.part_number == "RC0603-10K")
            .unwrap();
        assert_eq!((r.needed, r.each), (120, 0.02));
        assert!((r.cost - 2.4).abs() < 1e-9);
    }

    #[test]
    fn the_lowest_quantity_is_bought_when_fewer_are_needed() {
        let p = parse_prices(PRICES).unwrap();
        let c = cost(&bom(), &p, 2, 0.0).unwrap();
        let u = c.lines.iter().find(|l| l.part_number == "STM32").unwrap();
        assert_eq!((u.needed, u.bought), (2, 10));
        assert!((u.cost - 35.0).abs() < 1e-9);
        assert!(c
            .text("EUR")
            .contains("8 more than needed: the lowest quantity sold"));
    }

    #[test]
    fn the_spares_are_added_and_rounded_up() {
        let p = parse_prices(PRICES).unwrap();
        let c = cost(&bom(), &p, 10, 5.0).unwrap();
        let r = c
            .lines
            .iter()
            .find(|l| l.part_number == "RC0603-10K")
            .unwrap();
        // 30 pieces + 5% = 31.5 -> 32
        assert_eq!(r.needed, 32);
        let exact = cost(&bom(), &p, 20, 50.0).unwrap();
        let r = exact
            .lines
            .iter()
            .find(|l| l.part_number == "RC0603-10K")
            .unwrap();
        assert_eq!(r.needed, 90);
    }

    #[test]
    fn a_part_by_value_and_package_is_priced_and_one_without_a_price_is_told() {
        let p = parse_prices("value,package,price\n100n,0402,0.05\n").unwrap();
        let c = cost(&bom(), &p, 10, 0.0).unwrap();
        assert_eq!(c.lines.len(), 1);
        assert_eq!(c.lines[0].value, "100n");
        assert_eq!(
            c.unpriced,
            vec!["10k 0603".to_string(), "MCU TQFP-48".to_string()]
        );
        assert!((c.total - 0.5).abs() < 1e-9);
        assert!((c.per_board() - 0.05).abs() < 1e-9);
        assert!(c.text("").contains("no price for: 10k 0603, MCU TQFP-48"));
    }

    #[test]
    fn the_run_and_the_options_are_checked() {
        let p = parse_prices(PRICES).unwrap();
        assert!(cost(&bom(), &p, 0, 0.0).is_err());
        assert!(cost(&bom(), &p, MAX_BOARDS + 1, 0.0).is_err());
        assert!(cost(&bom(), &p, 1, -1.0).is_err());
        assert!(cost(&bom(), &p, 1, 1000.1).is_err());
        assert!(cost(&bom(), &p, 1, f64::NAN).is_err());
    }

    #[test]
    fn the_table_lists_the_dearest_first() {
        let p = parse_prices(PRICES).unwrap();
        let c = cost(&bom(), &p, 10, 0.0).unwrap();
        let costs: Vec<f64> = c.lines.iter().map(|l| l.cost).collect();
        assert!(costs.windows(2).all(|w| w[0] >= w[1]));
        let csv = c.csv();
        assert!(csv.starts_with("value,package,part_number,per_board,needed,bought,each,cost\n"));
        assert_eq!(csv.lines().count(), c.lines.len() + 1);
    }
}
