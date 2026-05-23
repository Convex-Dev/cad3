//! SHA3-256 value ID.
//!
//! `Sha3_256` is a ~400-byte stack struct (keccak state + rate buffer); it
//! does no heap allocation, so each call constructs a fresh hasher on the
//! stack. The real allocation win is avoiding a `Vec<u8>` for the encoding
//! — see [`Hash::streaming`] and the [`Sink`](crate::Sink) trait.

use std::fmt;

use sha3::{Digest, Sha3_256};

use crate::sink::Sink;

/// 32-byte SHA3-256 hash. Every CAD3 cell's value ID is the SHA3-256 of its
/// canonical encoding.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hash(pub [u8; 32]);

impl Hash {
    /// SHA3-256 of `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        Self::streaming(|sink| sink.write(bytes))
    }

    /// Compute a SHA3-256 by streaming bytes into a freshly stack-allocated
    /// hasher. Avoids materialising the input as a `Vec<u8>` — callers
    /// write tag bytes, payload bytes, and child value IDs directly to the
    /// hasher via the [`Sink`] trait.
    ///
    /// Trivially reentrant: each call has its own hasher on the stack.
    #[inline]
    pub fn streaming<F>(f: F) -> Self
    where
        F: FnOnce(&mut dyn Sink),
    {
        let mut hasher = Sha3_256::new();
        f(&mut hasher);
        let mut out = [0u8; 32];
        out.copy_from_slice(&hasher.finalize());
        Hash(out)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash(")?;
        for b in &self.0 {
            write!(f, "{:02x}", b)?;
        }
        write!(f, ")")
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in &self.0 {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}
