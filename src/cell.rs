//! The [`Cell`] enum — the central value type of this crate.
//!
//! Architectural rationale lives in `docs/CELL_DESIGN.md`. Headline:
//! `Cell` is a 16-byte value-type enum; small variants are inline bit
//! patterns, heavy variants hold `Arc<Inner>` for cheap-clone-by-bump
//! semantics. Sharing happens via the inner Arc inside the variant
//! payload, not via an outer `Arc<Cell>` wrapper.

use std::hash::{Hash as StdHash, Hasher};
use std::mem::discriminant;
use std::sync::Arc;

use bytes::Bytes;

use crate::types::{self, BlobInner, StringInner};
use crate::{tag, vlq, DecodeError, Hash, Sink, MAX_EMBEDDED_LENGTH};

/// Canonical IEEE 754 NaN bit pattern required by CAD3 §"Double".
const CANONICAL_NAN: u64 = 0x7ff8000000000000;
/// Maximum valid Unicode code point.
const MAX_CODE_POINT: u32 = 0x10FFFF;

/// A CAD3 cell.
///
/// 16-byte value-type enum: 1-byte discriminant + 7 bytes padding +
/// 8-byte payload slot. Small variants store data inline; heavy variants
/// hold `Arc<Inner>` to refcounted shared data.
///
/// `PartialEq`/`Eq`/`Hash` are implemented manually rather than derived
/// because [`Double`](Self::Double) carries `f64`, which doesn't impl
/// those — `Double` comparison and hashing use the bit representation
/// so all values (including NaN) have well-defined equality.
#[derive(Clone, Debug)]
pub enum Cell {
    /// The `nil` value (tag `0x00`).
    Nil,
    /// A one-byte flag value (tags `0xB0`–`0xBF`). Inner `u8` is the low
    /// nibble (`0`–`15`); `0`/`1` are the CVM booleans `false`/`true`.
    ByteFlag(u8),
    /// Signed integer in `i64` range (tags `0x10`–`0x18`).
    Long(i64),
    /// IEEE 754 double-precision floating point (tag `0x1D`). NaN
    /// canonicalises to `0x7ff8000000000000` on encode.
    Double(f64),
    /// Unicode scalar value (tags `0x3C`–`0x3E`).
    Char(char),
    /// Account address — extension value `0xEA` with VLQ payload.
    Address(u64),
    /// Arbitrary byte sequence (tag `0x31`). Currently only ≤ 4096-byte
    /// leaf form is supported; tree form lands with `Ref`.
    Blob(Arc<BlobInner>),
    /// UTF-8 string (tag `0x30`). Same leaf-form-only restriction as
    /// `Blob`. CAD3 does not enforce valid UTF-8.
    String(Arc<StringInner>),
}

// ---------------------------------------------------------------------------
// Manual equality / hash

impl PartialEq for Cell {
    fn eq(&self, other: &Self) -> bool {
        use Cell::*;
        match (self, other) {
            (Nil, Nil) => true,
            (ByteFlag(a), ByteFlag(b)) => a == b,
            (Long(a), Long(b)) => a == b,
            (Double(a), Double(b)) => a.to_bits() == b.to_bits(),
            (Char(a), Char(b)) => a == b,
            (Address(a), Address(b)) => a == b,
            // Arc fast-path: same allocation ⇒ equal. Otherwise compare
            // inner bytes via Arc<T>'s deref-PartialEq.
            (Blob(a), Blob(b)) => Arc::ptr_eq(a, b) || a == b,
            (String(a), String(b)) => Arc::ptr_eq(a, b) || a == b,
            _ => false,
        }
    }
}

impl Eq for Cell {}

impl StdHash for Cell {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            Cell::Nil => {}
            Cell::ByteFlag(n) => n.hash(state),
            Cell::Long(v) => v.hash(state),
            Cell::Double(v) => v.to_bits().hash(state),
            Cell::Char(c) => c.hash(state),
            Cell::Address(a) => a.hash(state),
            Cell::Blob(b) => b.hash(state),
            Cell::String(s) => s.hash(state),
        }
    }
}

// ---------------------------------------------------------------------------
// Constructors and accessors

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

    /// Construct a `Cell::Blob` from owned bytes.
    ///
    /// # Panics
    /// Panics if the payload exceeds 4096 bytes (tree form not yet
    /// implemented). Use [`Self::try_blob`] for a fallible variant.
    pub fn blob(bytes: impl Into<Bytes>) -> Self {
        Cell::Blob(Arc::new(BlobInner::leaf(bytes.into())))
    }

    /// Fallible counterpart of [`Self::blob`].
    pub fn try_blob(bytes: impl Into<Bytes>) -> Option<Self> {
        BlobInner::try_leaf(bytes.into()).map(|b| Cell::Blob(Arc::new(b)))
    }

    /// Construct a `Cell::String` from owned bytes (UTF-8 assumed but
    /// not validated).
    ///
    /// # Panics
    /// Panics if the payload exceeds 4096 bytes (tree form not yet
    /// implemented). Use [`Self::try_string`] for a fallible variant.
    pub fn string(bytes: impl Into<Bytes>) -> Self {
        Cell::String(Arc::new(StringInner::leaf(bytes.into())))
    }

    /// Fallible counterpart of [`Self::string`].
    pub fn try_string(bytes: impl Into<Bytes>) -> Option<Self> {
        StringInner::try_leaf(bytes.into()).map(|s| Cell::String(Arc::new(s)))
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

    /// Interpret this cell as an `i64`. Returns `None` for non-Long cells.
    pub const fn as_long(&self) -> Option<i64> {
        match self {
            Cell::Long(v) => Some(*v),
            _ => None,
        }
    }

    /// Interpret this cell as an `f64`. Returns `None` for non-Double cells.
    pub const fn as_double(&self) -> Option<f64> {
        match self {
            Cell::Double(v) => Some(*v),
            _ => None,
        }
    }

    /// Interpret this cell as a `char`. Returns `None` for non-Char cells.
    pub const fn as_char(&self) -> Option<char> {
        match self {
            Cell::Char(c) => Some(*c),
            _ => None,
        }
    }

    /// Interpret this cell as an address (`u64`). Returns `None` for
    /// non-Address cells.
    pub const fn as_address(&self) -> Option<u64> {
        match self {
            Cell::Address(a) => Some(*a),
            _ => None,
        }
    }

    /// Borrow the payload bytes of a Blob cell. Returns `None` for any
    /// other variant.
    pub fn as_blob(&self) -> Option<&Bytes> {
        match self {
            Cell::Blob(b) => Some(b.as_bytes()),
            _ => None,
        }
    }

    /// Borrow the payload bytes of a String cell. Returns `None` for any
    /// other variant. Bytes may not be valid UTF-8; use [`Self::as_str`]
    /// for UTF-8-validated access.
    pub fn as_string_bytes(&self) -> Option<&Bytes> {
        match self {
            Cell::String(s) => Some(s.as_bytes()),
            _ => None,
        }
    }

    /// View this cell's payload as a `&str` if it is a String with valid
    /// UTF-8 content.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Cell::String(s) => s.as_str(),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Encoding

impl Cell {
    /// Length of this cell's canonical encoding, in bytes.
    pub fn encoded_length(&self) -> usize {
        match self {
            Cell::Nil => 1,
            Cell::ByteFlag(_) => 1,
            Cell::Long(v) => 1 + min_signed_bytes(*v),
            Cell::Double(_) => 9,
            Cell::Char(c) => 1 + min_char_bytes(*c as u32),
            Cell::Address(a) => 1 + vlq::byte_len(*a),
            Cell::Blob(b) => b.encoded_length(),
            Cell::String(s) => s.encoded_length(),
        }
    }

    /// Whether this cell may be embedded in a parent cell's encoding.
    pub fn is_embedded(&self) -> bool {
        self.encoded_length() <= MAX_EMBEDDED_LENGTH
    }

    /// Write this cell's canonical encoding to `sink`.
    ///
    /// Generic over [`Sink`] so the same code path serves both encoding
    /// (sink = [`Vec<u8>`]) and value-ID hashing (sink = `Sha3_256`)
    /// without materialising the encoding to a buffer for hashing. The
    /// `?Sized` bound permits passing a `&mut dyn Sink` trait object.
    pub fn encode_into<S: Sink + ?Sized>(&self, sink: &mut S) {
        match self {
            Cell::Nil => sink.write(&[tag::NIL]),
            Cell::ByteFlag(n) => {
                debug_assert!(*n <= 0x0F);
                sink.write(&[tag::BYTE_FLAG_BASE | (*n & 0x0F)]);
            }
            Cell::Long(v) => encode_long(*v, sink),
            Cell::Double(v) => encode_double(*v, sink),
            Cell::Char(c) => encode_char(*c, sink),
            Cell::Address(a) => {
                sink.write(&[tag::ADDRESS]);
                vlq::encode(*a, sink);
            }
            Cell::Blob(b) => b.encode_into(sink),
            Cell::String(s) => s.encode_into(sink),
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
    /// For heavy variants (Blob, String, ...) this is cached on the
    /// underlying `*Inner` after first computation; subsequent calls
    /// from any clone of the cell return the cached value.
    pub fn value_id(&self) -> Hash {
        match self {
            // Heavy variants delegate to their Inner so the cache is used.
            Cell::Blob(b) => b.value_id(),
            Cell::String(s) => s.value_id(),
            // Small variants compute on demand — cheap (≤ 9 bytes to hash).
            _ => Hash::streaming(|sink| self.encode_into(sink)),
        }
    }
}

// ---------------------------------------------------------------------------
// Decoding

impl Cell {
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
    /// and the number of bytes consumed.
    pub fn decode_prefix(bytes: &[u8]) -> Result<(Self, usize), DecodeError> {
        let t = *bytes.first().ok_or(DecodeError::Empty)?;
        match t {
            tag::NIL => Ok((Cell::Nil, 1)),
            tag::LONG_BASE..=tag::LONG_MAX => decode_long(t, bytes),
            tag::DOUBLE => decode_double(bytes),
            tag::STRING => {
                let (payload, consumed) = types::decode_bytes_leaf(bytes)?;
                Ok((Cell::String(Arc::new(StringInner::leaf(payload))), consumed))
            }
            tag::BLOB => {
                let (payload, consumed) = types::decode_bytes_leaf(bytes)?;
                Ok((Cell::Blob(Arc::new(BlobInner::leaf(payload))), consumed))
            }
            0x3C..=0x3E => decode_char(t, bytes),
            0x3F => Err(DecodeError::ReservedTag(t)),
            tag::ADDRESS => decode_address(bytes),
            t if tag::category(t) == tag::BYTE_FLAG_BASE => Ok((Cell::ByteFlag(t & 0x0F), 1)),
            tag::ILLEGAL => Err(DecodeError::ReservedTag(t)),
            other => Err(DecodeError::UnknownTag(other)),
        }
    }
}

// ===========================================================================
// Variant-specific encode/decode helpers (small variants only — heavy
// variants delegate to their *Inner type's methods)

// --- Long -----------------------------------------------------------------

/// Minimum number of bytes needed to represent `v` in two's-complement
/// signed form. Zero needs zero bytes (just the tag).
fn min_signed_bytes(v: i64) -> usize {
    if v == 0 {
        return 0;
    }
    let bits = if v >= 0 {
        65 - v.leading_zeros() as usize
    } else {
        65 - (!v).leading_zeros() as usize
    };
    bits.div_ceil(8)
}

fn encode_long<S: Sink + ?Sized>(v: i64, sink: &mut S) {
    let n = min_signed_bytes(v);
    sink.write(&[tag::LONG_BASE + n as u8]);
    if n > 0 {
        let be = v.to_be_bytes();
        sink.write(&be[8 - n..]);
    }
}

fn decode_long(tag: u8, bytes: &[u8]) -> Result<(Cell, usize), DecodeError> {
    let n = (tag - tag::LONG_BASE) as usize;
    if bytes.len() < 1 + n {
        return Err(DecodeError::Truncated);
    }
    if n == 0 {
        return Ok((Cell::Long(0), 1));
    }
    let payload = &bytes[1..1 + n];
    if n > 1 {
        let top = payload[0];
        let next = payload[1];
        if (top == 0x00 && next & 0x80 == 0) || (top == 0xFF && next & 0x80 != 0) {
            return Err(DecodeError::NonMinimalLong);
        }
    }
    let sign_byte = if payload[0] & 0x80 != 0 { 0xFF } else { 0x00 };
    let mut extended = [sign_byte; 8];
    extended[8 - n..].copy_from_slice(payload);
    Ok((Cell::Long(i64::from_be_bytes(extended)), 1 + n))
}

// --- Double ---------------------------------------------------------------

fn encode_double<S: Sink + ?Sized>(v: f64, sink: &mut S) {
    let bits = if v.is_nan() {
        CANONICAL_NAN
    } else {
        v.to_bits()
    };
    sink.write(&[tag::DOUBLE]);
    sink.write(&bits.to_be_bytes());
}

fn decode_double(bytes: &[u8]) -> Result<(Cell, usize), DecodeError> {
    if bytes.len() < 9 {
        return Err(DecodeError::Truncated);
    }
    let bits = u64::from_be_bytes(bytes[1..9].try_into().unwrap());
    let v = f64::from_bits(bits);
    if v.is_nan() && bits != CANONICAL_NAN {
        return Err(DecodeError::NonCanonicalNaN);
    }
    Ok((Cell::Double(v), 9))
}

// --- Char -----------------------------------------------------------------

fn min_char_bytes(cp: u32) -> usize {
    if cp <= 0xFF {
        1
    } else if cp <= 0xFFFF {
        2
    } else {
        3
    }
}

fn encode_char<S: Sink + ?Sized>(c: char, sink: &mut S) {
    let cp = c as u32;
    let n = min_char_bytes(cp);
    sink.write(&[tag::CHAR_BASE + (n as u8 - 1)]);
    let be = cp.to_be_bytes();
    sink.write(&be[4 - n..]);
}

fn decode_char(tag: u8, bytes: &[u8]) -> Result<(Cell, usize), DecodeError> {
    let n = (tag - tag::CHAR_BASE + 1) as usize;
    if bytes.len() < 1 + n {
        return Err(DecodeError::Truncated);
    }
    let payload = &bytes[1..1 + n];
    if n > 1 && payload[0] == 0 {
        return Err(DecodeError::NonMinimalChar);
    }
    let mut be = [0u8; 4];
    be[4 - n..].copy_from_slice(payload);
    let cp = u32::from_be_bytes(be);
    if cp > MAX_CODE_POINT {
        return Err(DecodeError::InvalidCodePoint(cp));
    }
    let c = char::from_u32(cp).ok_or(DecodeError::InvalidCodePoint(cp))?;
    Ok((Cell::Char(c), 1 + n))
}

// --- Address --------------------------------------------------------------

fn decode_address(bytes: &[u8]) -> Result<(Cell, usize), DecodeError> {
    let (v, consumed) = vlq::decode(&bytes[1..])?;
    Ok((Cell::Address(v), 1 + consumed))
}
