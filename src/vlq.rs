//! VLQ (Variable Length Quantity) codec for unsigned counts.
//!
//! CAD3 uses VLQ for counts (vector length, map size, etc.) and for the
//! payload of extension values (e.g. Addresses). The format is base-128
//! with the high bit of each byte indicating continuation. See CAD3
//! §"VLQ Counts".
//!
//! Per the spec, only non-negative values up to 2⁶³ − 1 are valid in
//! CAD3 encodings, although the format technically supports
//! arbitrary-sized integers.

use crate::{DecodeError, Sink};

/// Maximum VLQ value permitted in CAD3 (63-bit unsigned).
pub const MAX_VALUE: u64 = i64::MAX as u64;

/// Number of bytes a VLQ-encoded value occupies.
#[inline]
pub const fn byte_len(v: u64) -> usize {
    if v == 0 {
        return 1;
    }
    let bits = 64 - v.leading_zeros() as usize;
    bits.div_ceil(7)
}

/// Write `v` to `sink` as a VLQ count.
pub fn encode<S: Sink + ?Sized>(v: u64, sink: &mut S) {
    let n = byte_len(v);
    let mut buf = [0u8; 10]; // u64::MAX needs 10 VLQ bytes; CAD3 caps at 9.
    for (i, slot) in buf.iter_mut().take(n).enumerate() {
        let shift = 7 * (n - 1 - i);
        let chunk = ((v >> shift) & 0x7F) as u8;
        let continuation = if i + 1 < n { 0x80 } else { 0x00 };
        *slot = chunk | continuation;
    }
    sink.write(&buf[..n]);
}

/// Decode a VLQ count from `bytes`. Returns the value and number of bytes
/// consumed.
pub fn decode(bytes: &[u8]) -> Result<(u64, usize), DecodeError> {
    let mut result: u64 = 0;
    for (i, &b) in bytes.iter().enumerate() {
        // Before applying the next 7-bit shift, check whether we'd lose
        // information from the top of u64. After 9 input bytes we've already
        // accumulated 63 bits; a 10th byte would push past u64.
        if (result >> 57) != 0 {
            return Err(DecodeError::VlqOverflow);
        }
        result = (result << 7) | (b & 0x7F) as u64;
        if b & 0x80 == 0 {
            // Terminator. Validate minimality (one valid length per value).
            if byte_len(result) != i + 1 {
                return Err(DecodeError::NonMinimalVlq);
            }
            if result > MAX_VALUE {
                return Err(DecodeError::VlqOverflow);
            }
            return Ok((result, i + 1));
        }
    }
    Err(DecodeError::UnterminatedVlq)
}
