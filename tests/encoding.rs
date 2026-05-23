//! Integration tests for byte-exact CAD3 encodings.

use cad3::{Cell, Sink, MAX_EMBEDDED_LENGTH};
use sha3::{Digest, Sha3_256};

fn sha3_256(bytes: &[u8]) -> [u8; 32] {
    let mut h = Sha3_256::new();
    h.update(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(&h.finalize());
    out
}

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
fn value_id_is_sha3_256_of_encoding() {
    for c in [Cell::Nil, Cell::FALSE, Cell::TRUE, Cell::byte_flag(0xA)] {
        let expected = sha3_256(&c.encoding());
        assert_eq!(c.value_id().0, expected);
    }
}

#[test]
fn distinct_cells_have_distinct_value_ids() {
    let mut ids: Vec<[u8; 32]> = (0u8..=15)
        .map(|n| Cell::byte_flag(n).value_id().0)
        .collect();
    ids.push(Cell::Nil.value_id().0);
    let count = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), count, "value IDs must be unique");
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

#[test]
fn value_id_does_not_require_intermediate_vec() {
    // Hash::streaming runs the encode_into closure straight into the
    // hasher. The result must equal the two-step (encoding + then-hash).
    for c in [Cell::Nil, Cell::FALSE, Cell::TRUE, Cell::byte_flag(0xC)] {
        let streamed = c.value_id();
        let buffered = sha3_256(&c.encoding());
        assert_eq!(streamed.0, buffered, "mismatch for {c:?}");
    }
}

#[test]
fn value_ids_consistent_across_threads() {
    // No shared mutable state — different threads must compute identical
    // value IDs for identical cells.
    let other = std::thread::spawn(|| {
        (0u8..=15)
            .map(|n| Cell::byte_flag(n).value_id().0)
            .collect::<Vec<_>>()
    })
    .join()
    .unwrap();
    let here: Vec<_> = (0u8..=15)
        .map(|n| Cell::byte_flag(n).value_id().0)
        .collect();
    assert_eq!(here, other);
}
