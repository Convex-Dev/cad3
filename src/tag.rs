//! CAD3 tag byte constants.
//!
//! See the [CAD3 specification](https://docs.convex.world/cad/003_encoding/)
//! for the full tag table. Only tags actually supported by this crate are
//! exposed; new tags are added as more types are implemented.

pub const NIL: u8 = 0x00;

pub const BYTE_FLAG_BASE: u8 = 0xB0;
pub const BYTE_FLAG_MASK: u8 = 0xF0;
pub const BYTE_FLAG_FALSE: u8 = 0xB0;
pub const BYTE_FLAG_TRUE: u8 = 0xB1;

pub const ILLEGAL: u8 = 0xFF;

/// Return the high-nibble category of a tag byte (e.g. `0xB0` for any byte
/// flag, `0x80` for any data structure).
#[inline]
pub const fn category(tag: u8) -> u8 {
    tag & 0xF0
}
