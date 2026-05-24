//! Decoding errors.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// Input was empty; CAD3 requires at least a tag byte.
    Empty,
    /// Encoding was valid but extra bytes followed. Per CAD3 §"Valid and
    /// Invalid Encodings" this makes the whole input invalid.
    TrailingBytes { consumed: usize, total: usize },
    /// Encoding terminated before the variant's payload was complete.
    Truncated,
    /// Tag byte is unknown to this implementation.
    UnknownTag(u8),
    /// Tag byte is explicitly reserved by the CAD3 spec or not yet
    /// permitted (e.g. `0xFF`, the 4-byte char tag `0x3F`).
    ReservedTag(u8),
    /// Long integer encoded with redundant leading sign bytes — CAD3
    /// requires the minimum-length representation.
    NonMinimalLong,
    /// Character encoded with a leading zero byte — could use fewer bytes.
    NonMinimalChar,
    /// VLQ count encoded with more bytes than necessary.
    NonMinimalVlq,
    /// Character code point invalid (> 0x10FFFF or a UTF-16 surrogate).
    InvalidCodePoint(u32),
    /// IEEE 754 NaN was not the canonical CAD3 bit pattern
    /// `0x7ff8000000000000`.
    NonCanonicalNaN,
    /// VLQ value exceeds the CAD3 63-bit limit.
    VlqOverflow,
    /// VLQ continuation bit was set at the end of input.
    UnterminatedVlq,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::Empty => write!(f, "empty encoding"),
            DecodeError::TrailingBytes { consumed, total } => write!(
                f,
                "trailing bytes after valid encoding (consumed {consumed} of {total})"
            ),
            DecodeError::Truncated => write!(f, "encoding truncated before payload complete"),
            DecodeError::UnknownTag(t) => write!(f, "unknown tag byte 0x{t:02x}"),
            DecodeError::ReservedTag(t) => write!(f, "reserved tag byte 0x{t:02x}"),
            DecodeError::NonMinimalLong => {
                write!(f, "long integer encoding has redundant leading sign byte")
            }
            DecodeError::NonMinimalChar => {
                write!(f, "character encoding has a leading zero byte")
            }
            DecodeError::NonMinimalVlq => write!(f, "VLQ count has redundant leading byte"),
            DecodeError::InvalidCodePoint(cp) => {
                write!(f, "invalid Unicode code point 0x{cp:x}")
            }
            DecodeError::NonCanonicalNaN => write!(
                f,
                "IEEE 754 NaN must be canonical bit pattern 0x7ff8000000000000"
            ),
            DecodeError::VlqOverflow => write!(f, "VLQ value exceeds 63-bit limit"),
            DecodeError::UnterminatedVlq => {
                write!(f, "VLQ continuation bit set at end of input")
            }
        }
    }
}

impl std::error::Error for DecodeError {}
