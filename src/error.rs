//! Decoding errors.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// Input was empty; CAD3 requires at least a tag byte.
    Empty,
    /// Encoding was valid but extra bytes followed. Per CAD3 §"Valid and
    /// Invalid Encodings" this makes the whole input invalid.
    TrailingBytes { consumed: usize, total: usize },
    /// Tag byte is unknown, reserved, or illegal for this implementation.
    UnknownTag(u8),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::Empty => write!(f, "empty encoding"),
            DecodeError::TrailingBytes { consumed, total } => write!(
                f,
                "trailing bytes after valid encoding (consumed {consumed} of {total})"
            ),
            DecodeError::UnknownTag(t) => {
                write!(f, "unknown or reserved tag byte 0x{t:02x}")
            }
        }
    }
}

impl std::error::Error for DecodeError {}
