//! [`BlobInner`] — payload of [`Cell::Blob`](crate::Cell::Blob).
//!
//! Implements CAD3 §"Blob" (tag `0x31`). A Blob is an immutable byte
//! sequence; in CAD3 it may be encoded either as a leaf (≤ 4096 bytes
//! inline) or as a tree of child Blobs (for larger payloads).
//!
//! **Current limitation:** only leaf-form is implemented. Constructing or
//! decoding a Blob whose payload exceeds 4096 bytes errors / panics
//! (depending on entry point). Tree form lands when [`Ref`] does — see
//! the migration plan in `docs/CELL_DESIGN.md`.
//!
//! [`Ref`]: crate::Ref

use std::hash::{Hash as StdHash, Hasher};
use std::sync::OnceLock;

use bytes::Bytes;

use crate::types::{bytes_leaf_encoded_length, encode_bytes_leaf, MAX_LEAF_BYTES};
use crate::{tag, Hash, Sink};

/// Payload of [`Cell::Blob`](crate::Cell::Blob).
///
/// Owns the byte data (refcounted via [`bytes::Bytes`], so multiple
/// `BlobInner`s can share the same underlying buffer if constructed from
/// the same `Bytes`). Caches its value ID lazily on the first
/// [`Self::value_id`] call.
///
/// Always handled behind `Arc<BlobInner>` in a `Cell` variant; cloning
/// `Cell::Blob` is one atomic refcount bump on the Arc.
pub struct BlobInner {
    bytes: Bytes,
    value_id: OnceLock<Hash>,
}

impl BlobInner {
    /// Construct a leaf BlobInner from the given bytes.
    ///
    /// # Panics
    /// Panics if `bytes.len() > 4096`. Use [`Self::try_leaf`] for a
    /// fallible variant. Tree-form construction (for larger payloads) is
    /// not yet implemented.
    pub fn leaf(bytes: Bytes) -> Self {
        Self::try_leaf(bytes).expect("BlobInner::leaf: payload exceeds 4096-byte leaf limit")
    }

    /// Fallible counterpart of [`Self::leaf`]. Returns `None` if the
    /// payload exceeds the 4096-byte leaf limit.
    pub fn try_leaf(bytes: Bytes) -> Option<Self> {
        if bytes.len() > MAX_LEAF_BYTES {
            None
        } else {
            Some(Self {
                bytes,
                value_id: OnceLock::new(),
            })
        }
    }

    /// Borrow the payload bytes.
    pub fn as_bytes(&self) -> &Bytes {
        &self.bytes
    }

    /// Number of bytes in the payload.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether the payload is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Length of the canonical CAD3 encoding (tag + VLQ length + payload).
    pub fn encoded_length(&self) -> usize {
        bytes_leaf_encoded_length(self.bytes.len())
    }

    /// Write the canonical encoding to `sink`.
    pub fn encode_into<S: Sink + ?Sized>(&self, sink: &mut S) {
        encode_bytes_leaf(tag::BLOB, &self.bytes, sink);
    }

    /// SHA3-256 of the canonical encoding — the CAD3 value ID.
    ///
    /// Computed lazily on first call; cached for subsequent calls and
    /// shared across every `Arc<BlobInner>` referring to this instance.
    pub fn value_id(&self) -> Hash {
        *self
            .value_id
            .get_or_init(|| Hash::streaming(|sink| self.encode_into(sink)))
    }
}

// Equality / hash deliberately ignore the cache — two BlobInners with the
// same bytes are equal regardless of whether the value_id has been
// computed yet on either side.

impl PartialEq for BlobInner {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

impl Eq for BlobInner {}

impl StdHash for BlobInner {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bytes.hash(state);
    }
}

impl std::fmt::Debug for BlobInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Show length and first few bytes for log brevity.
        let preview_len = self.bytes.len().min(16);
        let preview: Vec<String> = self.bytes[..preview_len]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let suffix = if self.bytes.len() > preview_len {
            "…"
        } else {
            ""
        };
        write!(
            f,
            "BlobInner({} bytes: {}{suffix})",
            self.bytes.len(),
            preview.join("")
        )
    }
}
