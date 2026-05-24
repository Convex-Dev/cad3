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
//! # Architecture
//!
//! See `docs/CELL_DESIGN.md` for the in-depth architectural notes.
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

pub mod cell;
pub mod error;
pub mod hash;
pub mod sink;
pub mod tag;

pub use cell::Cell;
pub use error::DecodeError;
pub use hash::Hash;
pub use sink::Sink;

/// Maximum encoded length of an embedded cell, per CAD3 §"Embedded
/// References". Cells longer than this must be referenced externally by
/// value ID.
pub const MAX_EMBEDDED_LENGTH: usize = 140;

/// Maximum encoded length of any single cell, per CAD3 §"Encoding".
pub const MAX_ENCODING_LENGTH: usize = 16383;

// Compile-time tripwire: Cell, Hash, and the public DecodeError must be
// safely shareable across threads. If any future variant accidentally
// introduces a non-Sync type (Rc, Cell<_>, RefCell<_>) the build fails here.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Cell>();
    assert_send_sync::<Hash>();
    assert_send_sync::<DecodeError>();
};
