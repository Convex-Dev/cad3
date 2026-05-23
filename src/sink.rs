//! Byte sink for cell encodings.
//!
//! Lets [`Cell::encode_into`](crate::Cell::encode_into) stream its canonical
//! bytes into any destination without an intermediate allocation. Two impls
//! ship: one for [`Vec<u8>`] (when the caller wants the encoding) and one
//! for [`Sha3_256`] (when the caller wants the value ID directly).
//!
//! Implementations are infallible — cell encodings have a known finite size
//! (≤ 16 383 bytes per cell, per CAD3 §"Encoding") and these sinks either
//! grow on demand or absorb bytes.

use sha3::digest::Digest;
use sha3::Sha3_256;

pub trait Sink {
    fn write(&mut self, bytes: &[u8]);
}

impl Sink for Vec<u8> {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
}

impl Sink for Sha3_256 {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        Digest::update(self, bytes);
    }
}
