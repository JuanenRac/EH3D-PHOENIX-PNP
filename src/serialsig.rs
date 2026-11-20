// EH3D-PHOENIX-PNP - the record of a board by its serial number, signed with a hash.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A customer who asks where the parts of board number 0042 came from is answered with a record: the serial, the fingerprints of the tables the board was made from (parts, feeders, lots),
//! each placed reference with its feeder and its lot, and the signature of the signature of the board before it, so that the records of a run form a chain in which nothing can be taken out
//! or changed without it showing. The signature is a SHA-256 of the whole record or, with a key, an HMAC-SHA-256 (for a record that must also say who made it). Nothing is sent anywhere
//! and the key is never written into the record. [`verify`] reads a record and says whether it is whole.

use crate::error::{Code, PnpError};
use crate::parts::{csv_field, read_records};
use crate::sha256::{hex, hmac_sha256, same, sha256, sha256_hex};

/// The first line of every record.
pub const MAGIC: &str = "phoenix-pnp serial record 1";
/// The most placed references of one record.
pub const MAX_ROWS: usize = 100_000;
/// The most characters of the serial and of a field of a row.
pub const MAX_FIELD: usize = 64;
/// The text that stands where there is no lots table and no board before.
pub const NONE: &str = "-";

fn bad(detail: impl Into<String>) -> PnpError {
    PnpError::new(Code::ImportBad, detail.into())
}

/// The fingerprint of a table: the SHA-256 of its text with the byte order mark taken off, every line ending written `\n`, the spaces at the end of a line cut and the empty lines at the
/// end dropped, so that the same table saved on Windows and on Linux has the same fingerprint.
pub fn fingerprint(text: &str) -> String {
    let text = text.trim_start_matches('\u{feff}');
    let mut lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    let mut norm = lines.join("\n");
    norm.push('\n');
    sha256_hex(norm.as_bytes())
}

/// One placed reference.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub reference: String,
    pub feeder: String,
    /// Empty when the feeder has no lot.
    pub lot: String,
}

/// What a record says.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub serial: String,
    pub parts: String,
    pub feeders: String,
    /// `NONE` when the board was made without a table of lots.
    pub lots: String,
    /// The signature of the record of the board before this one, or `NONE`.
    pub prev: String,
    pub rows: Vec<Row>,
}

/// How the record is signed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Sha256,
    HmacSha256,
}

impl Method {
    pub fn name(self) -> &'static str {
        match self {
            Method::Sha256 => "sha256",
            Method::HmacSha256 => "hmac-sha256",
        }
    }
}

/// A serial or a field of a row: up to `MAX_FIELD` characters, no control character and no space at either end (a table read back would not keep it).
fn field_ok(text: &str) -> bool {
    text.chars().count() <= MAX_FIELD && !text.chars().any(char::is_control) && text == text.trim()
}

fn hash_ok(text: &str) -> bool {
    text == NONE || (text.len() == 64 && text.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')))
}

/// The text that is signed: every line of the record but the signature.
pub fn body(rec: &Record) -> Result<String, PnpError> {
    if rec.serial.is_empty() || !field_ok(&rec.serial) {
        return Err(bad(format!(
            "the serial is from 1 to {MAX_FIELD} characters with no control character and no space at either end"
        )));
    }
    for (what, h) in [
        ("parts", &rec.parts),
        ("feeders", &rec.feeders),
        ("lots", &rec.lots),
        ("prev", &rec.prev),
    ] {
        if !hash_ok(h) {
            return Err(bad(format!(
                "{what}: a hash is 64 digits in lower case hexadecimal, or {NONE}"
            )));
        }
    }
    if rec.rows.len() > MAX_ROWS {
        return Err(bad(format!("more than {MAX_ROWS} references")));
    }
    let mut out = format!(
        "{MAGIC}\nserial: {}\nparts: {}\nfeeders: {}\nlots: {}\nprev: {}\nplaced: {}\nreference,feeder,lot\n",
        rec.serial,
        rec.parts,
        rec.feeders,
        rec.lots,
        rec.prev,
        rec.rows.len()
    );
    for r in &rec.rows {
        if r.reference.is_empty()
            || ![&r.reference, &r.feeder, &r.lot]
                .iter()
                .all(|f| field_ok(f))
        {
            return Err(bad(
                "a reference is needed, and each field is up to 64 characters with no control character and no space at either end",
            ));
        }
        out.push_str(&format!(
            "{},{},{}\n",
            csv_field(&r.reference),
            csv_field(&r.feeder),
            csv_field(&r.lot)
        ));
    }
    Ok(out)
}

fn signature(method: Method, key: Option<&[u8]>, body: &str) -> String {
    match (method, key) {
        (Method::HmacSha256, Some(k)) => hex(&hmac_sha256(k, body.as_bytes())),
        _ => hex(&sha256(body.as_bytes())),
    }
}

/// The record with its signature line. With a key the signature is an HMAC; an empty key is refused (it would sign nothing).
pub fn sign(rec: &Record, key: Option<&[u8]>) -> Result<String, PnpError> {
    if key.is_some_and(<[u8]>::is_empty) {
        return Err(bad("the key is empty"));
    }
    let method = if key.is_some() {
        Method::HmacSha256
    } else {
        Method::Sha256
    };
    let body = body(rec)?;
    let sig = signature(method, key, &body);
    Ok(format!("{body}signature: {} {sig}\n", method.name()))
}

/// What a whole record says.
#[derive(Debug, Clone, PartialEq)]
pub struct Verified {
    pub record: Record,
    pub method: Method,
    pub signature: String,
}

fn line_value<'a>(line: Option<&'a str>, name: &str) -> Result<&'a str, PnpError> {
    line.and_then(|l| l.strip_prefix(name))
        .and_then(|l| l.strip_prefix(": "))
        .ok_or_else(|| bad(format!("the line '{name}: ...' is missing or out of place")))
}

/// Reads a record, checks that it is the shape this program writes and that its signature is the one of its lines, and gives what it says. A record signed with a key needs the key, and a
/// record that is not signed with one is not accepted when a key is given (a signature taken off cannot pass for the signature of the key). `expect_prev` is the signature of the board
/// that should come before: when given, the record must name it.
pub fn verify(
    text: &str,
    key: Option<&[u8]>,
    expect_prev: Option<&str>,
) -> Result<Verified, PnpError> {
    if key.is_some_and(<[u8]>::is_empty) {
        return Err(bad("the key is empty"));
    }
    let text = text.trim_start_matches('\u{feff}');
    if !text.ends_with('\n') || text.contains('\r') {
        return Err(bad(
            "the record must be as it was written: lines ending in \\n",
        ));
    }
    let at = text[..text.len() - 1]
        .rfind('\n')
        .map(|i| i + 1)
        .ok_or_else(|| bad("the record has no signature line"))?;
    let (body_text, sig_line) = text.split_at(at);
    let sig_value = line_value(Some(sig_line.trim_end_matches('\n')), "signature")?;
    let (method_name, given) = sig_value
        .split_once(' ')
        .ok_or_else(|| bad("the signature line is 'signature: METHOD HASH'"))?;
    let method = match method_name {
        "sha256" => Method::Sha256,
        "hmac-sha256" => Method::HmacSha256,
        other => {
            return Err(bad(format!(
                "the method {other} of the signature is not known"
            )))
        }
    };
    match (method, key) {
        (Method::HmacSha256, None) => {
            return Err(bad("the record is signed with a key: give the key"))
        }
        (Method::Sha256, Some(_)) => {
            return Err(bad(
                "the record is not signed with a key, and a key was given",
            ))
        }
        _ => {}
    }
    if !hash_ok(given) || given == NONE {
        return Err(bad("the signature is not a hash"));
    }
    let mut lines = body_text.lines();
    if lines.next() != Some(MAGIC) {
        return Err(bad(
            "this is not a record of this program (or it is of a newer one)",
        ));
    }
    let serial = line_value(lines.next(), "serial")?.to_string();
    let parts = line_value(lines.next(), "parts")?.to_string();
    let feeders = line_value(lines.next(), "feeders")?.to_string();
    let lots = line_value(lines.next(), "lots")?.to_string();
    let prev = line_value(lines.next(), "prev")?.to_string();
    let placed: usize = line_value(lines.next(), "placed")?
        .parse()
        .map_err(|_| bad("placed is not a number"))?;
    if lines.next() != Some("reference,feeder,lot") {
        return Err(bad("the header of the references is missing"));
    }
    if placed > MAX_ROWS {
        return Err(bad(format!("more than {MAX_ROWS} references")));
    }
    let table: String = lines.map(|l| format!("{l}\n")).collect();
    let records = if table.is_empty() {
        Vec::new()
    } else {
        read_records(&table, ',').map_err(bad)?
    };
    let mut rows = Vec::with_capacity(records.len());
    for r in records {
        let [reference, feeder, lot] = r.as_slice() else {
            return Err(bad("a reference line has not three fields"));
        };
        rows.push(Row {
            reference: reference.clone(),
            feeder: feeder.clone(),
            lot: lot.clone(),
        });
    }
    if rows.len() != placed {
        return Err(bad(format!(
            "the record says {placed} references and has {}",
            rows.len()
        )));
    }
    let record = Record {
        serial,
        parts,
        feeders,
        lots,
        prev,
        rows,
    };
    // the record must be exactly what this program would write for what it says, and that is what was signed
    let rewritten = body(&record)?;
    let computed = signature(method, key, body_text);
    if rewritten != body_text || !same(&computed, given) {
        return Err(bad("the signature does not match: the record was changed"));
    }
    if let Some(p) = expect_prev {
        if !same(&record.prev, p) {
            return Err(bad(
                "the record does not follow the board that was expected (its prev differs)",
            ));
        }
    }
    Ok(Verified {
        record,
        method,
        signature: given.to_string(),
    })
}

/// The lines for a person that tell what a verified record says.
pub fn describe(v: &Verified) -> String {
    let r = &v.record;
    let mut lots: Vec<&str> = r
        .rows
        .iter()
        .filter(|x| !x.lot.is_empty())
        .map(|x| x.lot.as_str())
        .collect();
    lots.sort_unstable();
    lots.dedup();
    format!(
        "serial {}: {} references placed, {} lots, signed with {} ({})\nparts {}\nfeeders {}\nlots {}\nboard before {}\n",
        r.serial,
        r.rows.len(),
        lots.len(),
        v.method.name(),
        v.signature,
        r.parts,
        r.feeders,
        r.lots,
        r.prev
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(n: usize) -> Record {
        Record {
            serial: "0042".into(),
            parts: fingerprint("ref,x\nR1,1\n"),
            feeders: fingerprint("id,v\n1,2\n"),
            lots: NONE.into(),
            prev: NONE.into(),
            rows: (0..n)
                .map(|i| Row {
                    reference: format!("R{}", i + 1),
                    feeder: format!("F{}", i % 3),
                    lot: if i % 2 == 0 {
                        format!("L{i}")
                    } else {
                        String::new()
                    },
                })
                .collect(),
        }
    }

    #[test]
    fn a_signed_record_is_read_back_and_says_what_it_was_made_of() {
        for n in [0, 1, 7] {
            let rec = record(n);
            let text = sign(&rec, None).unwrap();
            let v = verify(&text, None, None).unwrap();
            assert_eq!(v.record, rec);
            assert_eq!(v.method, Method::Sha256);
            assert!(text.starts_with(MAGIC));
        }
    }

    #[test]
    fn the_same_record_gives_the_same_signature_and_another_serial_another() {
        let a = sign(&record(3), None).unwrap();
        assert_eq!(a, sign(&record(3), None).unwrap());
        let mut other = record(3);
        other.serial = "0043".into();
        assert_ne!(a, sign(&other, None).unwrap());
    }

    #[test]
    fn a_changed_reference_serial_or_hash_is_found() {
        let text = sign(&record(4), None).unwrap();
        for (from, to) in [
            ("R2,F1", "R2,F2"),
            ("serial: 0042", "serial: 0043"),
            ("L2", "L9"),
        ] {
            let changed = text.replacen(from, to, 1);
            assert_ne!(changed, text, "{from}");
            assert!(verify(&changed, None, None).is_err(), "{from}");
        }
    }

    #[test]
    fn every_byte_of_a_record_changed_is_found() {
        let text = sign(&record(3), None).unwrap();
        let bytes = text.as_bytes();
        for i in 0..bytes.len() {
            for sub in *b"x0 \n" {
                if bytes[i] == sub {
                    continue;
                }
                let mut changed = bytes.to_vec();
                changed[i] = sub;
                let changed = String::from_utf8(changed).unwrap();
                assert!(
                    verify(&changed, None, None).is_err(),
                    "byte {i} changed to {sub}"
                );
            }
        }
    }

    #[test]
    fn a_line_taken_out_or_added_is_found() {
        let text = sign(&record(4), None).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        for skip in 1..lines.len() {
            let mut kept = lines.clone();
            kept.remove(skip);
            let changed = kept.join("\n") + "\n";
            assert!(verify(&changed, None, None).is_err(), "line {skip}");
        }
        let longer = text.replacen("\nsignature", "\nR9,F1,L9\nsignature", 1);
        assert!(verify(&longer, None, None).is_err());
    }

    #[test]
    fn a_key_makes_the_signature_and_the_key_is_needed_to_check_it() {
        let rec = record(3);
        let text = sign(&rec, Some(b"secret key")).unwrap();
        assert!(text.contains("signature: hmac-sha256 "));
        assert!(!text.contains("secret"));
        assert!(verify(&text, Some(b"secret key"), None).is_ok());
        assert!(verify(&text, Some(b"another key"), None).is_err());
        assert!(verify(&text, None, None).is_err());
        let plain = sign(&rec, None).unwrap();
        assert!(verify(&plain, Some(b"secret key"), None).is_err());
        assert!(sign(&rec, Some(b"")).is_err());
        assert!(verify(&text, Some(b""), None).is_err());
        assert_ne!(
            text.rsplit(' ').next(),
            plain.rsplit(' ').next(),
            "a key changes the signature"
        );
    }

    #[test]
    fn records_of_a_run_make_a_chain() {
        let first = sign(&record(2), None).unwrap();
        let sig1 = verify(&first, None, None).unwrap().signature;
        let mut second = record(2);
        second.serial = "0043".into();
        second.prev = sig1.clone();
        let text2 = sign(&second, None).unwrap();
        assert!(verify(&text2, None, Some(&sig1)).is_ok());
        assert!(verify(&text2, None, Some(NONE)).is_err());
        assert!(verify(&first, None, Some(&sig1)).is_err());
        assert!(verify(&first, None, Some(NONE)).is_ok());
    }

    #[test]
    fn a_record_that_is_not_what_this_program_writes_is_refused() {
        let text = sign(&record(2), None).unwrap();
        assert!(verify(&text.replace('\n', "\r\n"), None, None).is_err());
        assert!(verify(text.trim_end(), None, None).is_err());
        assert!(verify("", None, None).is_err());
        assert!(verify("signature: sha256 00\n", None, None).is_err());
        assert!(verify(
            &text.replace(MAGIC, "phoenix-pnp serial record 2"),
            None,
            None
        )
        .is_err());
        assert!(verify(&text.replace("sha256", "md5"), None, None).is_err());
        let with_bom = format!("\u{feff}{text}");
        assert!(verify(&with_bom, None, None).is_ok());
    }

    #[test]
    fn a_record_cannot_be_made_of_bad_pieces() {
        let mut r = record(1);
        r.serial = String::new();
        assert!(body(&r).is_err());
        r.serial = "a".repeat(65);
        assert!(body(&r).is_err());
        r.serial = "a\nb".into();
        assert!(body(&r).is_err());
        let mut r = record(1);
        r.parts = "ABC".into();
        assert!(body(&r).is_err());
        let mut r = record(1);
        r.rows[0].reference = String::new();
        assert!(body(&r).is_err());
        let mut r = record(1);
        r.rows[0].lot = "x".repeat(65);
        assert!(body(&r).is_err());
    }

    #[test]
    fn a_field_with_a_space_at_either_end_is_refused_because_a_table_would_not_keep_it() {
        for bad in [" a", "a ", "\u{a0}a", "a\u{3000}"] {
            let mut r = record(1);
            r.serial = bad.into();
            assert!(body(&r).is_err(), "serial {bad:?}");
            let mut r = record(1);
            r.rows[0].lot = bad.into();
            assert!(body(&r).is_err(), "lot {bad:?}");
            let mut r = record(1);
            r.rows[0].reference = bad.into();
            assert!(body(&r).is_err(), "reference {bad:?}");
        }
        // a space inside is kept
        let mut r = record(1);
        r.rows[0].reference = "R 1".into();
        let text = sign(&r, None).unwrap();
        assert_eq!(verify(&text, None, None).unwrap().record, r);
    }

    #[test]
    fn references_with_commas_and_quotes_are_read_back() {
        let mut r = record(1);
        r.rows[0].reference = "R\"1\",a".into();
        r.rows[0].lot = "L;1".into();
        let text = sign(&r, None).unwrap();
        assert_eq!(verify(&text, None, None).unwrap().record, r);
    }

    #[test]
    fn the_fingerprint_does_not_care_for_line_endings_or_the_end_of_the_file() {
        let a = fingerprint("a,b\r\n1,2\r\n");
        assert_eq!(a, fingerprint("a,b\n1,2\n"));
        assert_eq!(a, fingerprint("\u{feff}a,b\n1,2  \n\n\n"));
        assert_ne!(a, fingerprint("a,b\n1,3\n"));
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn describe_counts_the_lots() {
        let text = sign(&record(5), None).unwrap();
        let v = verify(&text, None, None).unwrap();
        let said = describe(&v);
        assert!(said.contains("serial 0042: 5 references placed, 3 lots"));
        assert!(said.contains("signed with sha256"));
    }
}
