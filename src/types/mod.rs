//! Per-variant data types for the heavy [`Cell`](crate::Cell) variants.
//!
//! Each module here defines an `*Inner` struct that lives behind an
//! `Arc<...>` in a `Cell` variant payload. Inners own their data plus a
//! [`OnceLock<Hash>`](std::sync::OnceLock) value-ID cache; cloning an
//! `Arc<*Inner>` is a single atomic refcount bump and shares the cache.
//!
//! See `docs/CELL_DESIGN.md` for the architectural rationale.

pub mod blob;
pub mod string;

pub use blob::BlobInner;
pub use string::StringInner;

use bytes::Bytes;

use crate::{tag, vlq, DecodeError, Sink};

/// Largest payload, in bytes, that CAD3 permits in a leaf Blob/String
/// encoding. Larger payloads must be split into a tree of child Blobs.
/// See CAD3 §"Blob" and §"String".
pub const MAX_LEAF_BYTES: usize = 4096;

/// Encode a Blob/String leaf body: tag byte + VLQ length + payload bytes.
///
/// Used by both `BlobInner` and `StringInner` — the only difference
/// between their encodings is the tag byte (`0x31` vs `0x30`).
pub(crate) fn encode_bytes_leaf<S: Sink + ?Sized>(tag_byte: u8, payload: &[u8], sink: &mut S) {
    sink.write(&[tag_byte]);
    vlq::encode(payload.len() as u64, sink);
    sink.write(payload);
}

/// Decode a Blob/String leaf body, returning the payload and the total
/// number of bytes consumed (tag + length VLQ + payload).
///
/// Assumes the caller has already verified `source[0]` is the expected
/// tag byte (0x30 or 0x31). Rejects tree-form encodings (payload > 4096
/// bytes) with [`DecodeError::TreeNotImplemented`] for now.
pub(crate) fn decode_bytes_leaf(source: &[u8]) -> Result<(Bytes, usize), DecodeError> {
    let (count, vlq_len) = vlq::decode(&source[1..])?;
    let count_usize = count as usize;
    if count_usize > MAX_LEAF_BYTES {
        return Err(DecodeError::TreeNotImplemented);
    }
    let header_len = 1 + vlq_len;
    let total_len = header_len + count_usize;
    if source.len() < total_len {
        return Err(DecodeError::Truncated);
    }
    let payload = Bytes::copy_from_slice(&source[header_len..total_len]);
    Ok((payload, total_len))
}

/// Encoded byte length of a Blob/String leaf: tag + VLQ + payload.
pub(crate) fn bytes_leaf_encoded_length(payload_len: usize) -> usize {
    1 + vlq::byte_len(payload_len as u64) + payload_len
}

// Silence "unused" while CHAR_BASE etc. aren't routed through here yet.
#[allow(dead_code)]
const _SPEC_REFERENCE: u8 = tag::BLOB;
