//! CAD3 tag byte constants.
//!
//! See the [CAD3 specification](https://docs.convex.world/cad/003_encoding/)
//! for the full tag table. Only tags actually supported by this crate are
//! exposed; new tags are added as more types are implemented.

pub const NIL: u8 = 0x00;

// Numeric (0x1x)
/// Long base — the low nibble encodes the byte count (0..=8). Tag `0x10`
/// alone represents the integer zero.
pub const LONG_BASE: u8 = 0x10;
/// Maximum Long tag (8-byte integer).
pub const LONG_MAX: u8 = 0x18;
/// Big integer (length >= 9 bytes). Not yet implemented in this crate.
pub const BIG_INTEGER: u8 = 0x19;
/// IEEE 754 double-precision floating point.
pub const DOUBLE: u8 = 0x1D;

// Strings and Blobs (0x3x)
/// Character base — the low two bits encode the byte count (1..=4).
/// Tag `0x3C` = 1-byte (ASCII), `0x3D` = 2, `0x3E` = 3, `0x3F` = 4 (reserved).
pub const CHAR_BASE: u8 = 0x3C;

// Byte Flags (0xBx)
pub const BYTE_FLAG_BASE: u8 = 0xB0;
pub const BYTE_FLAG_MASK: u8 = 0xF0;
pub const BYTE_FLAG_FALSE: u8 = 0xB0;
pub const BYTE_FLAG_TRUE: u8 = 0xB1;

// Extension Values (0xEx)
pub const EXTENSION_VALUE_BASE: u8 = 0xE0;
/// Address (account index) — extension value with VLQ payload.
pub const ADDRESS: u8 = 0xEA;

pub const ILLEGAL: u8 = 0xFF;

/// Return the high-nibble category of a tag byte (e.g. `0xB0` for any byte
/// flag, `0x80` for any data structure).
#[inline]
pub const fn category(tag: u8) -> u8 {
    tag & 0xF0
}
