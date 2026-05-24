//! The [`Cell`] enum — the central value type of this crate.
//!
//! Architectural rationale lives in `docs/CELL_DESIGN.md`. Headline:
//! `Cell` is a 16-byte value-type enum once heavy variants land; small
//! variants are inline bit patterns, heavy variants hold `Arc<Inner>` for
//! cheap-clone-by-bump semantics. Sharing happens via the inner Arc inside
//! the variant payload, not via an outer `Arc<Cell>` wrapper.

use crate::{tag, DecodeError, Hash, Sink, MAX_EMBEDDED_LENGTH};

/// A CAD3 cell.
///
/// Cells are immutable values with a single canonical byte encoding. As
/// more CAD3 types are implemented this enum will grow new variants —
/// including heavy ones that hold `Arc<Inner>`, at which point `Cell` will
/// be 16 bytes of inline value (1-byte discriminant + 7 bytes padding +
/// 8 bytes payload). `Copy` is intentionally not derived so callers don't
/// grow accidental dependencies on implicit duplication; `Clone` is, and
/// stays cheap (bit copy for small variants, bit copy + atomic bump for
/// heavy variants once they land).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Cell {
    /// The `nil` value (tag `0x00`).
    Nil,
    /// A one-byte flag value (tags `0xB0`–`0xBF`). The inner `u8` holds the
    /// low nibble (`0`–`15`); `0` and `1` are the CVM booleans `false` and
    /// `true`.
    ByteFlag(u8),
}

impl Cell {
    /// CVM `false` (`0xB0`).
    pub const FALSE: Cell = Cell::ByteFlag(0);
    /// CVM `true` (`0xB1`).
    pub const TRUE: Cell = Cell::ByteFlag(1);

    /// Construct a byte flag from its low-nibble value.
    ///
    /// # Panics
    /// Panics if `value > 15`.
    pub const fn byte_flag(value: u8) -> Self {
        assert!(value <= 0x0F, "byte flag value must be in 0..=15");
        Cell::ByteFlag(value)
    }

    /// Construct a CVM boolean cell from a Rust `bool`.
    pub const fn bool(b: bool) -> Self {
        if b {
            Cell::TRUE
        } else {
            Cell::FALSE
        }
    }

    /// Interpret this cell as a CVM boolean. Returns `None` for any cell
    /// other than `0xB0` / `0xB1`.
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Cell::ByteFlag(0) => Some(false),
            Cell::ByteFlag(1) => Some(true),
            _ => None,
        }
    }

    /// Length of this cell's canonical encoding, in bytes.
    pub const fn encoded_length(&self) -> usize {
        match self {
            Cell::Nil => 1,
            Cell::ByteFlag(_) => 1,
        }
    }

    /// Whether this cell may be embedded in a parent cell's encoding.
    pub const fn is_embedded(&self) -> bool {
        self.encoded_length() <= MAX_EMBEDDED_LENGTH
    }

    /// Write this cell's canonical encoding to `sink`.
    ///
    /// Generic over [`Sink`] so the same code path serves both encoding
    /// (sink = [`Vec<u8>`]) and value-ID hashing (sink = `Sha3_256`),
    /// without ever materialising the encoding into a buffer for hashing.
    /// The `?Sized` bound permits passing a `&mut dyn Sink` trait object.
    pub fn encode_into<S: Sink + ?Sized>(&self, sink: &mut S) {
        match self {
            Cell::Nil => sink.write(&[tag::NIL]),
            Cell::ByteFlag(n) => {
                debug_assert!(*n <= 0x0F);
                sink.write(&[tag::BYTE_FLAG_BASE | (*n & 0x0F)]);
            }
        }
    }

    /// Append this cell's canonical encoding to `buf`. Convenience wrapper
    /// over [`Self::encode_into`].
    pub fn encode_to(&self, buf: &mut Vec<u8>) {
        self.encode_into(buf);
    }

    /// Return this cell's canonical encoding as a fresh `Vec<u8>`.
    pub fn encoding(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.encoded_length());
        self.encode_into(&mut buf);
        buf
    }

    /// SHA3-256 of this cell's canonical encoding — the CAD3 value ID.
    ///
    /// Streams the encoding directly into a stack-allocated SHA3-256
    /// hasher; does not allocate a `Vec` for the encoding. See
    /// [`Hash::streaming`] for the design.
    pub fn value_id(&self) -> Hash {
        Hash::streaming(|sink| self.encode_into(sink))
    }

    /// Decode a single cell from a byte slice. The slice must contain
    /// exactly one cell encoding; trailing bytes are rejected.
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let (cell, consumed) = Self::decode_prefix(bytes)?;
        if consumed != bytes.len() {
            return Err(DecodeError::TrailingBytes {
                consumed,
                total: bytes.len(),
            });
        }
        Ok(cell)
    }

    /// Decode a single cell from the start of `bytes`, returning the cell
    /// and the number of bytes consumed. Used internally by container
    /// types that decode child cells without requiring the parent buffer
    /// to end.
    pub fn decode_prefix(bytes: &[u8]) -> Result<(Self, usize), DecodeError> {
        let t = *bytes.first().ok_or(DecodeError::Empty)?;
        match t {
            tag::NIL => Ok((Cell::Nil, 1)),
            t if tag::category(t) == tag::BYTE_FLAG_BASE => Ok((Cell::ByteFlag(t & 0x0F), 1)),
            other => Err(DecodeError::UnknownTag(other)),
        }
    }
}
