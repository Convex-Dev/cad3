//! Convex CAD3 encoded data structures, in Rust.
//!
//! This crate is an in-progress port of the [CAD3 encoding
//! format](https://docs.convex.world/cad/003_encoding/) — the canonical wire
//! and storage format for all Convex values.
//!
//! Currently supported:
//!
//! - `nil` (tag `0x00`)
//! - Byte flags `0xB0`–`0xBF`, including the CVM booleans `false` (`0xB0`)
//!   and `true` (`0xB1`)
//!
//! Every cell has a single canonical byte encoding; its SHA3-256 hash is the
//! cell's [value ID](Cell::value_id).
//!
//! # Example
//!
//! ```
//! use cad3::Cell;
//!
//! assert_eq!(Cell::Nil.encoding(),   vec![0x00]);
//! assert_eq!(Cell::FALSE.encoding(), vec![0xB0]);
//! assert_eq!(Cell::TRUE.encoding(),  vec![0xB1]);
//!
//! let round_tripped = Cell::decode(&Cell::TRUE.encoding()).unwrap();
//! assert_eq!(round_tripped, Cell::TRUE);
//! ```

pub mod error;
pub mod hash;
pub mod tag;

pub use error::DecodeError;
pub use hash::Hash;

/// Maximum encoded length of an embedded cell, per CAD3 §"Embedded
/// References". Cells longer than this must be referenced externally by
/// value ID.
pub const MAX_EMBEDDED_LENGTH: usize = 140;

/// Maximum encoded length of any single cell, per CAD3 §"Encoding".
pub const MAX_ENCODING_LENGTH: usize = 16383;

/// A CAD3 cell.
///
/// Cells are immutable values with a single canonical byte encoding. As more
/// CAD3 types are implemented this enum will grow new variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

    /// Append this cell's canonical encoding to `buf`.
    pub fn encode_to(&self, buf: &mut Vec<u8>) {
        match self {
            Cell::Nil => buf.push(tag::NIL),
            Cell::ByteFlag(n) => {
                debug_assert!(*n <= 0x0F);
                buf.push(tag::BYTE_FLAG_BASE | (*n & 0x0F));
            }
        }
    }

    /// Return this cell's canonical encoding as a fresh `Vec<u8>`.
    pub fn encoding(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.encoded_length());
        self.encode_to(&mut buf);
        buf
    }

    /// SHA3-256 of this cell's canonical encoding — the CAD3 value ID.
    pub fn value_id(&self) -> Hash {
        Hash::of(&self.encoding())
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
    /// and the number of bytes consumed. Used internally by container types
    /// that decode child cells without requiring the parent buffer to end.
    pub fn decode_prefix(bytes: &[u8]) -> Result<(Self, usize), DecodeError> {
        let t = *bytes.first().ok_or(DecodeError::Empty)?;
        match t {
            tag::NIL => Ok((Cell::Nil, 1)),
            t if tag::category(t) == tag::BYTE_FLAG_BASE => Ok((Cell::ByteFlag(t & 0x0F), 1)),
            other => Err(DecodeError::UnknownTag(other)),
        }
    }
}
