//! Integration tests for byte-exact CAD3 encodings and SHA3-256 value IDs.

use cad3::{Cell, DecodeError, Hash, Sink, MAX_EMBEDDED_LENGTH};

// ===========================================================================
// Nil

#[test]
fn nil_encoding_is_single_zero_byte() {
    assert_eq!(Cell::Nil.encoding(), vec![0x00]);
    assert_eq!(Cell::Nil.encoded_length(), 1);
}

#[test]
fn nil_round_trip() {
    let enc = Cell::Nil.encoding();
    assert_eq!(Cell::decode(&enc).unwrap(), Cell::Nil);
}

// ===========================================================================
// ByteFlag / booleans

#[test]
fn boolean_encodings_match_spec() {
    assert_eq!(Cell::FALSE.encoding(), vec![0xB0]);
    assert_eq!(Cell::TRUE.encoding(), vec![0xB1]);
}

#[test]
fn bool_helpers() {
    assert_eq!(Cell::bool(false), Cell::FALSE);
    assert_eq!(Cell::bool(true), Cell::TRUE);
    assert_eq!(Cell::FALSE.as_bool(), Some(false));
    assert_eq!(Cell::TRUE.as_bool(), Some(true));
    assert_eq!(Cell::Nil.as_bool(), None);
    assert_eq!(Cell::byte_flag(7).as_bool(), None);
}

#[test]
fn byte_flag_full_range_round_trip() {
    for n in 0u8..=15 {
        let c = Cell::byte_flag(n);
        let enc = c.encoding();
        assert_eq!(enc, vec![0xB0 | n]);
        assert_eq!(Cell::decode(&enc).unwrap(), c);
    }
}

// ===========================================================================
// Long

#[test]
fn long_zero_is_single_tag_byte() {
    assert_eq!(Cell::Long(0).encoding(), vec![0x10]);
}

#[test]
fn long_spec_example_19() {
    // From CAD3 spec: integer 19 → 0x1113.
    assert_eq!(Cell::Long(19).encoding(), vec![0x11, 0x13]);
}

#[test]
fn long_small_positives() {
    assert_eq!(Cell::Long(1).encoding(), vec![0x11, 0x01]);
    assert_eq!(Cell::Long(127).encoding(), vec![0x11, 0x7F]);
    // 128 doesn't fit in a signed byte — needs 2 bytes with leading 0.
    assert_eq!(Cell::Long(128).encoding(), vec![0x12, 0x00, 0x80]);
    assert_eq!(Cell::Long(255).encoding(), vec![0x12, 0x00, 0xFF]);
}

#[test]
fn long_small_negatives() {
    assert_eq!(Cell::Long(-1).encoding(), vec![0x11, 0xFF]);
    assert_eq!(Cell::Long(-128).encoding(), vec![0x11, 0x80]);
    // -129 needs 2 bytes (most-significant 0xFF, then 0x7F).
    assert_eq!(Cell::Long(-129).encoding(), vec![0x12, 0xFF, 0x7F]);
}

#[test]
fn long_extremes() {
    let max = Cell::Long(i64::MAX).encoding();
    assert_eq!(max[0], 0x18);
    assert_eq!(&max[1..], &[0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
    let min = Cell::Long(i64::MIN).encoding();
    assert_eq!(min[0], 0x18);
    assert_eq!(&min[1..], &[0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn long_round_trip_many_values() {
    for v in [
        0i64,
        1,
        -1,
        127,
        -128,
        128,
        -129,
        255,
        -256,
        65535,
        -65536,
        1 << 30,
        -(1 << 30),
        i64::MAX,
        i64::MIN,
        1234567890,
        -987654321,
    ] {
        let c = Cell::Long(v);
        let enc = c.encoding();
        assert_eq!(Cell::decode(&enc).unwrap(), c, "value {v}");
        assert_eq!(c.encoded_length(), enc.len(), "length mismatch for {v}");
    }
}

#[test]
fn long_rejects_non_minimal_encoding() {
    // 0x12 0x00 0x19 = "use 2 bytes to hold 25" — could use 1 byte (0x11 0x19).
    assert_eq!(
        Cell::decode(&[0x12, 0x00, 0x19]),
        Err(DecodeError::NonMinimalLong)
    );
    // 0x12 0xFF 0xFF = "use 2 bytes to hold -1" — could use 1 byte.
    assert_eq!(
        Cell::decode(&[0x12, 0xFF, 0xFF]),
        Err(DecodeError::NonMinimalLong)
    );
}

#[test]
fn long_rejects_truncated_encoding() {
    // 0x12 says "2 bytes follow" but only 1 byte present.
    assert_eq!(Cell::decode(&[0x12, 0x00]), Err(DecodeError::Truncated));
    assert_eq!(Cell::decode(&[0x18]), Err(DecodeError::Truncated));
}

// ===========================================================================
// Double

#[test]
fn double_zero_round_trip() {
    let enc = Cell::Double(0.0).encoding();
    assert_eq!(enc[0], 0x1D);
    assert_eq!(&enc[1..], &[0u8; 8]);
    assert_eq!(Cell::decode(&enc).unwrap(), Cell::Double(0.0));
}

#[test]
fn double_negative_zero_distinct_from_positive_zero() {
    let pos = Cell::Double(0.0).encoding();
    let neg = Cell::Double(-0.0).encoding();
    assert_ne!(
        pos, neg,
        "+0.0 and -0.0 are distinct cells (bit patterns differ)"
    );
}

#[test]
fn double_round_trip_finite_values() {
    for v in [
        1.0_f64,
        -1.0,
        std::f64::consts::PI,
        f64::MIN,
        f64::MAX,
        f64::EPSILON,
    ] {
        let c = Cell::Double(v);
        assert_eq!(Cell::decode(&c.encoding()).unwrap(), c, "value {v}");
    }
}

#[test]
fn double_infinities_round_trip() {
    let pi = Cell::Double(f64::INFINITY);
    let ni = Cell::Double(f64::NEG_INFINITY);
    assert_eq!(Cell::decode(&pi.encoding()).unwrap(), pi);
    assert_eq!(Cell::decode(&ni.encoding()).unwrap(), ni);
}

#[test]
fn double_nan_canonicalised_on_encode() {
    // Spec: NaN MUST encode as 0x1D 7F F8 00 00 00 00 00 00.
    let canonical = [0x1D, 0x7F, 0xF8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(Cell::Double(f64::NAN).encoding(), canonical);
    // Any other NaN bit pattern also encodes canonically.
    let weird_nan = f64::from_bits(0x7FFA000000000000); // a non-canonical NaN
    assert!(weird_nan.is_nan());
    assert_eq!(Cell::Double(weird_nan).encoding(), canonical);
}

#[test]
fn double_rejects_non_canonical_nan_on_decode() {
    let weird = [0x1D, 0x7F, 0xFA, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(Cell::decode(&weird), Err(DecodeError::NonCanonicalNaN));
}

#[test]
fn double_rejects_truncated() {
    assert_eq!(
        Cell::decode(&[0x1D, 0, 0, 0, 0]),
        Err(DecodeError::Truncated)
    );
    assert_eq!(Cell::decode(&[0x1D]), Err(DecodeError::Truncated));
}

// ===========================================================================
// Char

#[test]
fn char_ascii_one_byte() {
    assert_eq!(Cell::Char('A').encoding(), vec![0x3C, 0x41]);
    assert_eq!(Cell::Char('\0').encoding(), vec![0x3C, 0x00]);
    // 0xFF still fits in 1 byte (unsigned).
    assert_eq!(Cell::Char('\u{FF}').encoding(), vec![0x3C, 0xFF]);
}

#[test]
fn char_two_byte() {
    // 0x100 (Ā) is the first code point requiring 2 bytes.
    assert_eq!(Cell::Char('\u{0100}').encoding(), vec![0x3D, 0x01, 0x00]);
    assert_eq!(Cell::Char('中').encoding(), vec![0x3D, 0x4E, 0x2D]);
    assert_eq!(Cell::Char('\u{FFFF}').encoding(), vec![0x3D, 0xFF, 0xFF]);
}

#[test]
fn char_three_byte() {
    // 0x10000 is the first code point requiring 3 bytes.
    assert_eq!(
        Cell::Char('\u{10000}').encoding(),
        vec![0x3E, 0x01, 0x00, 0x00]
    );
    // 🎉 = U+1F389
    assert_eq!(Cell::Char('🎉').encoding(), vec![0x3E, 0x01, 0xF3, 0x89]);
    // Unicode max: U+10FFFF
    assert_eq!(
        Cell::Char('\u{10FFFF}').encoding(),
        vec![0x3E, 0x10, 0xFF, 0xFF]
    );
}

#[test]
fn char_round_trip_diverse_values() {
    for c in ['\0', 'A', 'ñ', 'Ā', '中', '🎉', '\u{10FFFF}'] {
        let cell = Cell::Char(c);
        assert_eq!(Cell::decode(&cell.encoding()).unwrap(), cell, "char {c:?}");
    }
}

#[test]
fn char_rejects_non_minimal() {
    // 0x3D 00 41 — 'A' encoded as 2 bytes when 1 would do.
    assert_eq!(
        Cell::decode(&[0x3D, 0x00, 0x41]),
        Err(DecodeError::NonMinimalChar)
    );
    // 0x3E 00 01 00 — could fit in 2 bytes.
    assert_eq!(
        Cell::decode(&[0x3E, 0x00, 0x01, 0x00]),
        Err(DecodeError::NonMinimalChar)
    );
}

#[test]
fn char_rejects_codepoint_beyond_unicode_max() {
    // 0x3E 11 00 00 = 0x110000, which is > Unicode max 0x10FFFF.
    assert_eq!(
        Cell::decode(&[0x3E, 0x11, 0x00, 0x00]),
        Err(DecodeError::InvalidCodePoint(0x110000))
    );
}

#[test]
fn char_rejects_reserved_four_byte_tag() {
    // 0x3F is reserved (4-byte char, not currently possible).
    assert_eq!(
        Cell::decode(&[0x3F, 0x00, 0x00, 0x00, 0x00]),
        Err(DecodeError::ReservedTag(0x3F))
    );
}

// ===========================================================================
// Address

#[test]
fn address_zero() {
    assert_eq!(Cell::Address(0).encoding(), vec![0xEA, 0x00]);
}

#[test]
fn address_vlq_boundaries() {
    assert_eq!(Cell::Address(127).encoding(), vec![0xEA, 0x7F]);
    assert_eq!(Cell::Address(128).encoding(), vec![0xEA, 0x81, 0x00]);
    assert_eq!(Cell::Address(16383).encoding(), vec![0xEA, 0xFF, 0x7F]);
    assert_eq!(
        Cell::Address(16384).encoding(),
        vec![0xEA, 0x81, 0x80, 0x00]
    );
}

#[test]
fn address_round_trip() {
    for a in [
        0u64,
        1,
        127,
        128,
        16384,
        1_000_000,
        1u64 << 40,
        i64::MAX as u64,
    ] {
        let c = Cell::Address(a);
        assert_eq!(Cell::decode(&c.encoding()).unwrap(), c, "address {a}");
    }
}

#[test]
fn address_rejects_non_minimal_vlq() {
    // 0x80 0x00 = 0 encoded with 2 bytes; should be a single 0x00.
    assert_eq!(
        Cell::decode(&[0xEA, 0x80, 0x00]),
        Err(DecodeError::NonMinimalVlq)
    );
}

#[test]
fn address_rejects_overflow() {
    // 10 bytes of 0xFF ... 0x7F encodes a value past 2^63 - 1.
    let mut bytes = vec![0xEA];
    bytes.extend(std::iter::repeat_n(0xFF, 9));
    bytes.push(0x7F);
    assert_eq!(Cell::decode(&bytes), Err(DecodeError::VlqOverflow));
}

// ===========================================================================
// Common encoding behaviour

#[test]
fn empty_input_rejected() {
    assert_eq!(Cell::decode(&[]), Err(DecodeError::Empty));
}

#[test]
fn trailing_bytes_rejected() {
    // CAD3 §"Valid and Invalid Encodings": extra bytes after a valid
    // encoding make the whole input invalid.
    matches!(
        Cell::decode(&[0x00, 0x00]),
        Err(DecodeError::TrailingBytes { .. })
    );
    matches!(
        Cell::decode(&[0xB1, 0xFF]),
        Err(DecodeError::TrailingBytes { .. })
    );
}

#[test]
fn unknown_tag_rejected() {
    // 0x40 is in a reserved category not implemented yet.
    assert_eq!(Cell::decode(&[0x40]), Err(DecodeError::UnknownTag(0x40)));
}

#[test]
fn illegal_tag_rejected() {
    // 0xFF is explicitly illegal per CAD3.
    assert_eq!(Cell::decode(&[0xFF]), Err(DecodeError::ReservedTag(0xFF)));
}

#[test]
fn big_integer_tag_not_yet_supported() {
    // 0x19 (BigInt) is in the spec but not implemented in this crate yet.
    assert_eq!(
        Cell::decode(&[0x19, 0x00]),
        Err(DecodeError::UnknownTag(0x19))
    );
}

#[test]
fn all_supported_cells_are_embedded() {
    for c in [
        Cell::Nil,
        Cell::FALSE,
        Cell::TRUE,
        Cell::byte_flag(0xF),
        Cell::Long(i64::MAX),
        Cell::Double(f64::MAX),
        Cell::Char('🎉'),
        Cell::Address(u64::MAX >> 1),
    ] {
        assert!(c.is_embedded(), "{c:?} should be embeddable");
        assert!(c.encoded_length() <= MAX_EMBEDDED_LENGTH);
    }
}

#[test]
fn encode_to_appends_to_existing_buffer() {
    let mut buf = vec![0xAA, 0xBB];
    Cell::TRUE.encode_to(&mut buf);
    assert_eq!(buf, vec![0xAA, 0xBB, 0xB1]);
}

#[test]
fn encode_into_works_with_vec_and_dyn_sink() {
    let cell = Cell::Long(42);

    let mut v: Vec<u8> = Vec::new();
    cell.encode_into(&mut v);
    assert_eq!(v, vec![0x11, 0x2A]);

    let mut v2: Vec<u8> = Vec::new();
    let sink: &mut dyn Sink = &mut v2;
    cell.encode_into(sink);
    assert_eq!(v2, vec![0x11, 0x2A]);
}

// ===========================================================================
// SHA3 + value IDs

/// FIPS 202 (SHA-3) Appendix B test vectors. Pin the SHA3-256
/// implementation itself.
#[test]
fn sha3_256_matches_nist_test_vectors() {
    assert_eq!(
        Hash::of(b"").to_string(),
        "a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a"
    );
    assert_eq!(
        Hash::of(b"abc").to_string(),
        "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"
    );
}

/// Pinned CAD3 value IDs. Computed independently with
/// `printf '\xNN…' | openssl dgst -sha3-256`. Any change is a wire-
/// incompatible break.
#[test]
fn cell_value_ids_match_independent_implementation() {
    assert_eq!(
        Cell::Nil.value_id().to_string(),
        "5d53469f20fef4f8eab52b88044ede69c77a6a68a60728609fc4a65ff531e7d0"
    );
    assert_eq!(
        Cell::FALSE.value_id().to_string(),
        "07da05bf823af1825541e8d90acd6ed29e582b8c9fae66fd99bb8ddf458e4454"
    );
    assert_eq!(
        Cell::TRUE.value_id().to_string(),
        "a6124adec80e7954c0bd1293f8ed316cb360a920936a1a20cb07d180f2a34d12"
    );
}

#[test]
fn value_id_streaming_matches_buffered() {
    for c in [
        Cell::Nil,
        Cell::FALSE,
        Cell::TRUE,
        Cell::byte_flag(0xC),
        Cell::Long(42),
        Cell::Long(i64::MIN),
        Cell::Double(std::f64::consts::PI),
        Cell::Char('🎉'),
        Cell::Address(1337),
    ] {
        assert_eq!(c.value_id(), Hash::of(&c.encoding()), "mismatch for {c:?}");
    }
}

#[test]
fn distinct_cells_have_distinct_value_ids() {
    let cells = [
        Cell::Nil,
        Cell::FALSE,
        Cell::TRUE,
        Cell::byte_flag(0xF),
        Cell::Long(0),
        Cell::Long(1),
        Cell::Long(-1),
        Cell::Double(0.0),
        Cell::Char('A'),
        Cell::Address(0),
        Cell::Address(1),
    ];
    let mut ids: Vec<_> = cells.iter().map(|c| c.value_id()).collect();
    let n = ids.len();
    ids.sort_by_key(|h| h.0);
    ids.dedup();
    assert_eq!(ids.len(), n, "value IDs must be unique");
}

#[test]
fn value_ids_consistent_across_threads() {
    let other = std::thread::spawn(|| {
        (0u8..=15)
            .map(|n| Cell::byte_flag(n).value_id())
            .collect::<Vec<_>>()
    })
    .join()
    .unwrap();
    let here: Vec<_> = (0u8..=15).map(|n| Cell::byte_flag(n).value_id()).collect();
    assert_eq!(here, other);
}
