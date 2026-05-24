//! Shared test helpers — generic invariants any [`Cell`] must satisfy, and
//! a corpus of cells covering every implemented variant + boundary values.
//!
//! Mirrors the Java port's `convex.core.data.GenTestFormat.doFormatTest` —
//! one function that, given any cell, asserts every cross-cutting CAD3
//! property. New cell types add an entry to [`corpus`] and inherit all the
//! invariants for free.
//!
//! Rust note: `tests/` is special — each file under it is a separate
//! integration-test *crate*. To share helpers between them, code goes in
//! `tests/common/mod.rs` (note: `mod.rs`, not a sibling file) and each
//! consumer does `mod common; use common::*;` at the top of its file. This
//! file is therefore NOT itself a test crate — it has no `#[test]`
//! functions; it's a library of helpers consumed by its siblings.

#![allow(dead_code)] // each consuming crate uses only the parts it needs

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash as StdHash, Hasher};

use cad3::{Cell, DecodeError, Hash, MAX_EMBEDDED_LENGTH, MAX_ENCODING_LENGTH};

/// Run every cross-cutting CAD3 invariant on `cell`. Panics with a clear
/// message identifying both the invariant and the offending cell.
///
/// Invariants verified:
///
/// 1.  `encoded_length()` equals the actual encoded byte count
/// 2.  encoding length is ≤ [`MAX_ENCODING_LENGTH`]
/// 3.  Encoding is deterministic (same bytes every call)
/// 4.  Round-trip: `decode(encode(c)) == c`
/// 5.  `value_id() == SHA3-256(encoding())`
/// 6.  `value_id()` is deterministic
/// 7.  `is_embedded()` agrees with `encoded_length() <= MAX_EMBEDDED_LENGTH`
/// 8.  `PartialEq` reflexive: `c == c`
/// 9.  Clone equality: `c.clone() == c`
/// 10. Canonical re-encoding: `decode(encode(c)).encoding() == encode(c)`
/// 11. Round-tripped cell has the same value_id
/// 12. `Hash` consistent with `Eq`: equal cells hash equally
/// 13. Trailing bytes after a valid encoding are rejected
pub fn assert_cell_invariants(cell: &Cell) {
    // 1. encoded_length() matches actual encoding length
    let enc = cell.encoding();
    assert_eq!(
        cell.encoded_length(),
        enc.len(),
        "encoded_length() lies about encoding size for {cell:?}"
    );

    // 2. Encoding within CAD3 spec maximum
    assert!(
        enc.len() <= MAX_ENCODING_LENGTH,
        "encoding {} bytes exceeds CAD3 max {MAX_ENCODING_LENGTH} for {cell:?}",
        enc.len()
    );

    // 3. Encoding is deterministic
    assert_eq!(
        cell.encoding(),
        enc,
        "encoding not deterministic for {cell:?}"
    );

    // 4. Round trip — decode produces an equal cell
    let decoded = Cell::decode(&enc)
        .unwrap_or_else(|e| panic!("decode failed for {cell:?}: {e} (bytes {enc:02x?})"));
    assert_eq!(&decoded, cell, "round-trip mismatch for {cell:?}");

    // 5. Value ID is SHA3-256 of the canonical encoding
    assert_eq!(
        cell.value_id(),
        Hash::of(&enc),
        "value_id != SHA3-256(encoding) for {cell:?}"
    );

    // 6. Value ID is deterministic
    assert_eq!(
        cell.value_id(),
        cell.value_id(),
        "value_id not deterministic for {cell:?}"
    );

    // 7. is_embedded agrees with the 140-byte rule
    let should_embed = cell.encoded_length() <= MAX_EMBEDDED_LENGTH;
    assert_eq!(
        cell.is_embedded(),
        should_embed,
        "is_embedded ({}) inconsistent with encoded_length ({}) for {cell:?}",
        cell.is_embedded(),
        cell.encoded_length()
    );

    // 8. PartialEq is reflexive. Using a distinct reference to dodge
    //    `clippy::eq_op` — testing that == returns true for c == c IS the
    //    point of this invariant.
    let same: &Cell = cell;
    assert!(cell == same, "self-equality fails for {cell:?}");

    // 9. Clone equality
    let cloned = cell.clone();
    assert_eq!(&cloned, cell, "clone != original for {cell:?}");

    // 10. Re-encoding a decoded cell gives the same bytes (canonical form)
    assert_eq!(
        decoded.encoding(),
        enc,
        "decoded cell re-encodes differently for {cell:?}"
    );

    // 11. Decoded cell has same value ID
    assert_eq!(
        decoded.value_id(),
        cell.value_id(),
        "decoded cell has different value_id for {cell:?}"
    );

    // 12. Hash trait must agree with Eq (Rust trait contract)
    let mut h1 = DefaultHasher::new();
    let mut h2 = DefaultHasher::new();
    cell.hash(&mut h1);
    cloned.hash(&mut h2);
    assert_eq!(
        h1.finish(),
        h2.finish(),
        "Hash inconsistent with Eq for {cell:?}"
    );

    // 13. Trailing bytes after a valid encoding are rejected (CAD3 §"Valid
    //     and Invalid Encodings")
    let mut with_extra = enc.clone();
    with_extra.push(0x00);
    let err = Cell::decode(&with_extra)
        .err()
        .unwrap_or_else(|| panic!("trailing bytes not rejected for {cell:?}"));
    assert!(
        matches!(err, DecodeError::TrailingBytes { .. }),
        "expected TrailingBytes error for {cell:?}, got {err:?}"
    );
}

/// Boundary-rich corpus covering every implemented `Cell` variant. Add new
/// variants here as they're implemented; every entry then inherits the
/// full invariant suite.
pub fn corpus() -> Vec<Cell> {
    let mut cells = vec![
        // Nil
        Cell::Nil,
        // ByteFlag — boundaries + the CVM booleans
        Cell::byte_flag(0),  // CVM false
        Cell::byte_flag(1),  // CVM true
        Cell::byte_flag(7),  // middle
        Cell::byte_flag(15), // max
        // Long — every byte-width boundary, plus extremes
        Cell::Long(0),
        Cell::Long(1),
        Cell::Long(-1),
        Cell::Long(127),
        Cell::Long(-128),
        Cell::Long(128),
        Cell::Long(-129),
        Cell::Long(32_767),
        Cell::Long(-32_768),
        Cell::Long(32_768),
        Cell::Long(-32_769),
        Cell::Long(1 << 30),
        Cell::Long(-(1 << 30)),
        Cell::Long(i64::MAX),
        Cell::Long(i64::MIN),
        // Double — zeros, ones, extremes, infinities, NaN
        Cell::Double(0.0),
        Cell::Double(-0.0),
        Cell::Double(1.0),
        Cell::Double(-1.0),
        Cell::Double(std::f64::consts::PI),
        Cell::Double(std::f64::consts::E),
        Cell::Double(f64::MIN),
        Cell::Double(f64::MAX),
        Cell::Double(f64::MIN_POSITIVE),
        Cell::Double(f64::EPSILON),
        Cell::Double(f64::INFINITY),
        Cell::Double(f64::NEG_INFINITY),
        Cell::Double(f64::NAN),
        // Char — width boundaries + Unicode extremes
        Cell::Char('\0'),
        Cell::Char('A'),
        Cell::Char('\u{7F}'),
        Cell::Char('\u{80}'),
        Cell::Char('\u{FF}'),
        Cell::Char('\u{100}'),
        Cell::Char('中'),
        Cell::Char('\u{FFFF}'),
        Cell::Char('\u{10000}'),
        Cell::Char('🎉'),
        Cell::Char('\u{10FFFF}'),
        // Address — VLQ width boundaries + extremes
        Cell::Address(0),
        Cell::Address(127),
        Cell::Address(128),
        Cell::Address(16_383),
        Cell::Address(16_384),
        Cell::Address(1 << 35),
        Cell::Address(i64::MAX as u64),
    ];

    // A handful of "interesting" Long values from real-world ranges.
    for v in [1_234_567_890_i64, -987_654_321, 0x55AA_55AA, -0x55AA_55AA] {
        cells.push(Cell::Long(v));
    }

    cells
}
