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
// Blob

#[test]
fn empty_blob_encoding() {
    // Empty Blob: tag 0x31, length 0 (VLQ 0x00), no payload.
    assert_eq!(Cell::blob(bytes::Bytes::new()).encoding(), vec![0x31, 0x00]);
}

#[test]
fn small_blob_encoding() {
    // 3-byte blob: tag, VLQ length 3, then the bytes.
    let c = Cell::blob(bytes::Bytes::from_static(&[0x01, 0x02, 0x03]));
    assert_eq!(c.encoding(), vec![0x31, 0x03, 0x01, 0x02, 0x03]);
}

#[test]
fn blob_round_trip_at_size_boundaries() {
    for size in [0usize, 1, 127, 128, 200, 4095, 4096] {
        let data = vec![(size & 0xFF) as u8; size];
        let original = Cell::blob(bytes::Bytes::from(data));
        let enc = original.encoding();
        let decoded = Cell::decode(&enc).unwrap();
        assert_eq!(decoded, original, "round trip failed at size {size}");
    }
}

#[test]
fn blob_construction_too_large_panics() {
    let result = std::panic::catch_unwind(|| {
        Cell::blob(bytes::Bytes::from(vec![0u8; 4097]));
    });
    assert!(result.is_err(), "Cell::blob > 4096 bytes should panic");
}

#[test]
fn blob_try_construction_too_large_returns_none() {
    assert!(Cell::try_blob(bytes::Bytes::from(vec![0u8; 4097])).is_none());
}

#[test]
fn blob_decode_rejects_tree_form() {
    // A leaf blob with declared length 5000 (> 4096) is the start of a
    // tree-form encoding, which we don't yet support.
    // VLQ for 5000: 5000 = 0b1001110001000 → 0xA7 0x08
    let mut bytes = vec![0x31, 0xA7, 0x08];
    bytes.extend(std::iter::repeat_n(0u8, 5000));
    assert_eq!(Cell::decode(&bytes), Err(DecodeError::TreeNotImplemented));
}

#[test]
fn blob_decode_rejects_truncated() {
    // Header says 10 bytes but only 3 follow.
    assert_eq!(
        Cell::decode(&[0x31, 0x0A, 0x01, 0x02, 0x03]),
        Err(DecodeError::Truncated)
    );
}

#[test]
fn blob_as_blob_accessor() {
    let payload = bytes::Bytes::from_static(b"hello");
    let c = Cell::blob(payload.clone());
    assert_eq!(c.as_blob(), Some(&payload));
    assert_eq!(Cell::Nil.as_blob(), None);
}

#[test]
fn blob_clone_is_arc_bump_not_deep_copy() {
    use std::sync::Arc;
    let original = Cell::blob(bytes::Bytes::from(vec![0xAB; 4096]));
    let cloned = original.clone();
    let Cell::Blob(a) = &original else {
        panic!();
    };
    let Cell::Blob(b) = &cloned else {
        panic!();
    };
    // Same heap allocation — clone bumped the Arc, didn't copy BlobInner.
    assert!(Arc::ptr_eq(a, b));
}

#[test]
fn blob_value_id_cached_on_inner() {
    use std::sync::Arc;
    let cell = Cell::blob(bytes::Bytes::from(vec![1, 2, 3, 4]));
    let id1 = cell.value_id();
    let id2 = cell.value_id();
    assert_eq!(id1, id2);
    // The Inner's cache is shared across clones.
    let cloned = cell.clone();
    let id3 = cloned.value_id();
    assert_eq!(id1, id3);
    // Sanity: matches the buffered SHA3 path.
    assert_eq!(id1, Hash::of(&cell.encoding()));
    // Cells with the same content but different Arc allocations also match.
    let independent = Cell::blob(bytes::Bytes::from(vec![1, 2, 3, 4]));
    let Cell::Blob(a) = &cell else { panic!() };
    let Cell::Blob(b) = &independent else {
        panic!()
    };
    assert!(!Arc::ptr_eq(a, b));
    assert_eq!(independent.value_id(), id1);
}

// ===========================================================================
// String

#[test]
fn empty_string_encoding() {
    assert_eq!(
        Cell::string(bytes::Bytes::new()).encoding(),
        vec![0x30, 0x00]
    );
}

#[test]
fn string_hello_encoding() {
    // "Hi" = 0x48 0x69 → tag 0x30, length 2, bytes.
    assert_eq!(
        Cell::string(bytes::Bytes::from_static(b"Hi")).encoding(),
        vec![0x30, 0x02, b'H', b'i']
    );
}

#[test]
fn string_utf8_multibyte_encoding() {
    // "中" = 3 UTF-8 bytes E4 B8 AD
    let payload = "中";
    let c = Cell::string(bytes::Bytes::from_static(payload.as_bytes()));
    assert_eq!(c.encoding(), vec![0x30, 0x03, 0xE4, 0xB8, 0xAD]);
}

#[test]
fn string_round_trip_at_size_boundaries() {
    for size in [0usize, 1, 127, 128, 200, 4095, 4096] {
        // Use ASCII so length-in-bytes == length-in-chars and validation passes.
        let data = vec![b'a'; size];
        let original = Cell::string(bytes::Bytes::from(data));
        let decoded = Cell::decode(&original.encoding()).unwrap();
        assert_eq!(decoded, original, "round trip failed at size {size}");
    }
}

#[test]
fn string_construction_too_large_panics() {
    let result = std::panic::catch_unwind(|| {
        Cell::string(bytes::Bytes::from(vec![b'x'; 4097]));
    });
    assert!(result.is_err(), "Cell::string > 4096 bytes should panic");
}

#[test]
fn string_decode_rejects_tree_form() {
    let mut bytes = vec![0x30, 0xA7, 0x08]; // tag + VLQ(5000)
    bytes.extend(std::iter::repeat_n(b'x', 5000));
    assert_eq!(Cell::decode(&bytes), Err(DecodeError::TreeNotImplemented));
}

#[test]
fn string_invalid_utf8_accepted_but_as_str_returns_none() {
    // Per CAD3, the encoding does NOT enforce UTF-8 — we accept it but
    // `as_str()` returns None.
    let invalid: &[u8] = &[0xFF, 0xFE, 0xFD];
    let cell = Cell::string(bytes::Bytes::from_static(invalid));
    assert_eq!(cell.as_string_bytes().unwrap().as_ref(), invalid);
    assert_eq!(cell.as_str(), None);
    // Round-trips fine despite invalid bytes.
    assert_eq!(Cell::decode(&cell.encoding()).unwrap(), cell);
}

#[test]
fn string_as_str_accessor() {
    let cell = Cell::string(bytes::Bytes::from_static(b"Hello"));
    assert_eq!(cell.as_str(), Some("Hello"));
    assert_eq!(Cell::Nil.as_str(), None);
    assert_eq!(
        Cell::blob(bytes::Bytes::from_static(b"Hello")).as_str(),
        None
    );
}

#[test]
fn blob_and_string_distinct_value_ids_for_same_bytes() {
    // Same byte payload, different tag → different encoding → different value ID.
    let payload = bytes::Bytes::from_static(b"hello");
    let blob = Cell::blob(payload.clone());
    let string = Cell::string(payload);
    assert_ne!(blob, string);
    assert_ne!(blob.value_id(), string.value_id());
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
