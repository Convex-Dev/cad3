//! Convex CAD3 encoded data structures, in Rust.
//!
//! This crate is an in-progress port of the [CAD3 encoding
//! format](https://docs.convex.world/cad/003_encoding/) — the canonical wire
//! and storage format for all Convex values.
//!
//! Currently supported:
//!
//! - `nil` (tag `0x00`)
//! - Long integers (tags `0x10`–`0x18`)
//! - Doubles (tag `0x1D`, with canonical NaN)
//! - Strings (tag `0x30`, leaf only — ≤ 4096 bytes)
//! - Blobs (tag `0x31`, leaf only — ≤ 4096 bytes)
//! - Characters (tags `0x3C`–`0x3E`)
//! - Byte flags (tags `0xB0`–`0xBF`), including the CVM booleans
//! - Addresses (extension value `0xEA`)
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
//! assert_eq!(Cell::Nil.encoding(),                  vec![0x00]);
//! assert_eq!(Cell::FALSE.encoding(),                vec![0xB0]);
//! assert_eq!(Cell::Long(19).encoding(),             vec![0x11, 0x13]);
//! assert_eq!(Cell::Address(0).encoding(),           vec![0xEA, 0x00]);
//! assert_eq!(Cell::string("Hi").encoding(),         vec![0x30, 0x02, b'H', b'i']);
//! assert_eq!(Cell::blob(vec![1, 2, 3]).encoding(),  vec![0x31, 0x03, 1, 2, 3]);
//!
//! let round_tripped = Cell::decode(&Cell::Long(-1).encoding()).unwrap();
//! assert_eq!(round_tripped, Cell::Long(-1));
//! ```

pub mod cell;
pub mod error;
pub mod hash;
pub mod sink;
pub mod tag;
pub mod types;
pub mod vlq;

pub use cell::Cell;
pub use error::DecodeError;
pub use hash::Hash;
pub use sink::Sink;
pub use types::{BlobInner, StringInner};

/// Maximum encoded length of an embedded cell, per CAD3 §"Embedded
/// References". Cells longer than this must be referenced externally by
/// value ID.
pub const MAX_EMBEDDED_LENGTH: usize = 140;

/// Maximum encoded length of any single cell, per CAD3 §"Encoding".
pub const MAX_ENCODING_LENGTH: usize = 16383;

// Compile-time tripwire: Cell, Hash, DecodeError, and the Inner types
// behind Arc must all be safely shareable across threads. If any future
// variant accidentally introduces a non-Sync type (Rc, Cell<_>, RefCell<_>)
// the build fails here.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Cell>();
    assert_send_sync::<Hash>();
    assert_send_sync::<DecodeError>();
    assert_send_sync::<BlobInner>();
    assert_send_sync::<StringInner>();
};

// Pin Cell at 16 bytes (1-byte discriminant + 7 bytes padding + 8-byte
// payload). Any new variant whose payload exceeds 8 bytes — or whose
// alignment exceeds 8 — bloats Cell beyond 16 and fails the build here.
// See docs/CELL_DESIGN.md §"Cell, byte by byte".
const _: () = assert!(
    std::mem::size_of::<Cell>() == 16,
    "Cell must remain 16 bytes; see docs/CELL_DESIGN.md"
);
