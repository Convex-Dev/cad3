//! Integration tests for byte-exact CAD3 encodings and SHA3-256 value IDs.

use cad3::{Cell, Hash, Sink, MAX_EMBEDDED_LENGTH};

#[test]
fn nil_encoding_is_single_zero_byte() {
    assert_eq!(Cell::Nil.encoding(), vec![0x00]);
    assert_eq!(Cell::Nil.encoded_length(), 1);
}

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

#[test]
fn nil_round_trip() {
    let enc = Cell::Nil.encoding();
    assert_eq!(Cell::decode(&enc).unwrap(), Cell::Nil);
}

#[test]
fn empty_input_rejected() {
    assert!(Cell::decode(&[]).is_err());
}

#[test]
fn trailing_bytes_rejected() {
    // CAD3 §"Valid and Invalid Encodings": extra bytes after a valid
    // encoding make the whole input invalid.
    assert!(Cell::decode(&[0x00, 0x00]).is_err());
    assert!(Cell::decode(&[0xB1, 0xFF]).is_err());
}

#[test]
fn unknown_or_reserved_tags_rejected() {
    // 0xFF is explicitly illegal.
    assert!(Cell::decode(&[0xFF]).is_err());
    // 0x40 is in a reserved category not implemented yet.
    assert!(Cell::decode(&[0x40]).is_err());
    // 0x10 (Integer) is in the spec but not implemented in this crate yet.
    assert!(Cell::decode(&[0x10]).is_err());
}

#[test]
fn all_supported_cells_are_embedded() {
    for c in [Cell::Nil, Cell::FALSE, Cell::TRUE, Cell::byte_flag(0xF)] {
        assert!(c.is_embedded());
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
    // Same encoding logic, two destinations. Vec path is monomorphised;
    // dyn-Sink path goes through vtable.
    let cell = Cell::TRUE;

    let mut v: Vec<u8> = Vec::new();
    cell.encode_into(&mut v);
    assert_eq!(v, vec![0xB1]);

    let mut v2: Vec<u8> = Vec::new();
    let sink: &mut dyn Sink = &mut v2;
    cell.encode_into(sink);
    assert_eq!(v2, vec![0xB1]);
}

/// FIPS 202 (SHA-3) Appendix B test vectors, verified by openssl. These
/// pin the SHA3-256 implementation itself — independent of anything the
/// cad3 crate does. If these break, the underlying hash crate has changed.
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

/// Pinned CAD3 value IDs for the cells this crate currently supports.
/// Computed independently with `printf '\xNN' | openssl dgst -sha3-256`.
/// Any change here is a wire-incompatible break.
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
    // Cell::value_id() streams encoding bytes straight into the hasher.
    // Hash::of(&encoding) takes the buffered route through a Vec. Both
    // must produce identical hashes — this is the contract the Sink
    // abstraction is built on.
    for c in [Cell::Nil, Cell::FALSE, Cell::TRUE, Cell::byte_flag(0xC)] {
        assert_eq!(c.value_id(), Hash::of(&c.encoding()), "mismatch for {c:?}");
    }
}

#[test]
fn distinct_cells_have_distinct_value_ids() {
    let mut ids: Vec<Hash> = (0u8..=15).map(|n| Cell::byte_flag(n).value_id()).collect();
    ids.push(Cell::Nil.value_id());
    let count = ids.len();
    ids.sort_by_key(|h| h.0);
    ids.dedup();
    assert_eq!(ids.len(), count, "value IDs must be unique");
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
