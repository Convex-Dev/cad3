//! SHA3-256 hash used as the CAD3 value ID of a cell.

use sha3::{Digest, Sha3_256};
use std::fmt;

/// 32-byte SHA3-256 hash. Every CAD3 cell's value ID is the SHA3-256 of its
/// canonical encoding.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hash(pub [u8; 32]);

impl Hash {
    /// Compute the SHA3-256 of `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        let mut hasher = Sha3_256::new();
        hasher.update(bytes);
        let out = hasher.finalize();
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&out);
        Hash(arr)
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
