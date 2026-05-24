//! [`StringInner`] — payload of [`Cell::String`](crate::Cell::String).
//!
//! Implements CAD3 §"String" (tag `0x30`). A String is logically a UTF-8
//! sequence of characters, but CAD3 stores it as a byte sequence with
//! UTF-8 *assumed* and *not enforced* — the spec leaves invalid-UTF-8
//! handling to applications.
//!
//! The encoding format is identical to [`BlobInner`](super::BlobInner)
//! except for the tag byte (`0x30` instead of `0x31`); children of tree-
//! form strings are themselves Blobs, allowing structural sharing.
//!
//! **Current limitation:** only leaf-form (≤ 4096 bytes) is implemented;
//! tree form lands when `Ref` does.

use std::hash::{Hash as StdHash, Hasher};
use std::sync::OnceLock;

use bytes::Bytes;

use crate::types::{bytes_leaf_encoded_length, encode_bytes_leaf, MAX_LEAF_BYTES};
use crate::{tag, Hash, Sink};

/// Payload of [`Cell::String`](crate::Cell::String).
///
/// Owns the underlying bytes (UTF-8 assumed); caches its value ID
/// lazily. Sharing semantics are identical to
/// [`BlobInner`](super::BlobInner) — cloning `Cell::String` is one atomic
/// refcount bump on the wrapping `Arc<StringInner>`.
pub struct StringInner {
    bytes: Bytes,
    value_id: OnceLock<Hash>,
}

impl StringInner {
    /// Construct a leaf StringInner from the given bytes. The bytes
    /// SHOULD be valid UTF-8 but CAD3 does not enforce this; see
    /// [`Self::as_str`] for safe UTF-8 access.
    ///
    /// # Panics
    /// Panics if `bytes.len() > 4096`. Use [`Self::try_leaf`] for a
    /// fallible variant. Tree-form construction is not yet implemented.
    pub fn leaf(bytes: Bytes) -> Self {
        Self::try_leaf(bytes).expect("StringInner::leaf: payload exceeds 4096-byte leaf limit")
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

    /// Borrow the underlying bytes (may not be valid UTF-8).
    pub fn as_bytes(&self) -> &Bytes {
        &self.bytes
    }

    /// View the payload as a `&str` if it is valid UTF-8.
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.bytes).ok()
    }

    /// Number of bytes in the payload (not Unicode characters).
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
        encode_bytes_leaf(tag::STRING, &self.bytes, sink);
    }

    /// SHA3-256 of the canonical encoding — the CAD3 value ID.
    pub fn value_id(&self) -> Hash {
        *self
            .value_id
            .get_or_init(|| Hash::streaming(|sink| self.encode_into(sink)))
    }
}

impl PartialEq for StringInner {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

impl Eq for StringInner {}

impl StdHash for StringInner {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bytes.hash(state);
    }
}

impl std::fmt::Debug for StringInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(s) = self.as_str() {
            // Show valid UTF-8 quoted, truncated if long.
            let preview = if s.chars().count() > 32 {
                let head: String = s.chars().take(32).collect();
                format!("{head}…")
            } else {
                s.to_string()
            };
            write!(f, "StringInner({:?})", preview)
        } else {
            write!(f, "StringInner({} non-UTF-8 bytes)", self.bytes.len())
        }
    }
}
