# Cell Design

Architectural design for the `Cell` type in this Rust port of CAD3. Aimed
at a reader fluent in Java/the JVM `convex-core` codebase but who does not
normally read Rust — Rust-specific machinery is called out as it appears.

## Goal

CAD3 is built on **efficient structural sharing**. Two cells that contain
the same sub-value share that sub-value's storage; updating one cell
produces a new cell whose unchanged sub-trees share `Arc`s with the
original; extracting a chunk of bytes from a parent encoding does not
copy those bytes. Without structural sharing the lattice is theoretically
correct but useless in practice.

### Secondary goal

A fresh `Cell` value pays no overhead beyond the data it actually
carries:

- **Small variants** pay zero — no heap, no refcount. A
  `Cell::Long(42)` constructed as a function-local intermediate is 16
  bytes on the stack, full stop.
- **Heavy variants** pay exactly one heap allocation for the `Inner`
  (the inner is variable-size and must live somewhere). No refcount on
  the *initial* construction; the refcount comes from `Arc::new` but it
  starts at 1, no atomic bump.
- **Refcounting is for sharing.** Cloning a Cell bumps the inner Arc;
  dropping decrements. Cells that aren't cloned never touch an atomic
  beyond their initial alloc and final free.

## The shape

A `Cell` *is* the value — 16 bytes, value-type, `Clone`. Cloning is
cheap for every variant:

- Small variants: a 16-byte bit copy.
- Heavy variants: a 16-byte bit copy plus one atomic increment on the
  inner Arc.

Heavy variants hold `Arc<Inner>` directly inside the enum payload. The
Arc lives *in* the Cell, doing the one job we need it for — making
Cell clone cheap and letting two Cells share their data. There is no
outer `Arc<Cell>` anywhere; Refs hold Cells by value.

```rust
pub enum Cell {                       // 16-byte value-type enum
    Nil,
    ByteFlag(u8),
    Long(i64),
    Double(f64),
    Char(char),
    Address(u64),
    Blob(Arc<BlobInner>),             // cheap clone via Arc bump
    Vector(Arc<VectorInner>),
    // …
}

pub enum Ref {                        // 40 bytes, holds Cell by value
    Embedded(Cell),
    Branch(Hash),
}
```

This is the core insight: **for immutable lattice data, "Cell is a
shared handle" is what user code wants 99% of the time, and Arc lets
us match that without growing the type or adding layers of wrapping.**

## Why `Arc<Inner>` inside the variant (not `Box`, not `Arc<Cell>`)

There were three candidate shapes for heavy variants. Walking through
what each costs settles the choice:

| Design                                  | `Cell::clone` cost (heavy)        | Allocs to construct shared blob | Layers of refcount |
|-----------------------------------------|------------------------------------|----------------------------------|---------------------|
| `Cell::Blob(Box<BlobInner>)` + `Ref::Embedded(Arc<Cell>)` | Box deep-clone — heap alloc + deep copy of Inner | 2 (BlobInner + Arc<Cell> header) | 2 |
| `Arc<CellData>` wrapping the whole enum | atomic bump                        | 1 (CellData with Inner inlined)  | 1, but per-Cell heap cost ~104 B even for Nil |
| **`Cell::Blob(Arc<BlobInner>)`**        | **atomic bump**                    | **1**                            | **1** |

`Arc<Inner>` wins on every axis. The earlier worry about "Arc inside
Cell" was conflating it with "Arc wrapping Cell." This is the inverse —
the Arc is *part of* the Cell, where the variable-size data already
needs to live.

### Why not `Box`

`Box<T>` is single-owner heap promotion. `Box<T>::clone()` requires
`T: Clone` and **deep-copies** — fresh heap allocation, deep clone of
the Inner. For heavy CAD3 variants (Blob, Vector, Map) this means every
`Cell::clone()` allocates and duplicates. Hostile to the common
"extract from one structure, store into another" pattern.

### Why not `Arc<CellData>` around the whole enum

That makes every Cell heap-allocated, including Nil. Heap cost per Cell
is uniform ~104 B regardless of variant. Small variants pay heavily for
sharing they don't use. Cache-unfriendly for `Vec<Cell>` iteration
(every element is a pointer chase).

### Why `Arc<Inner>` works cleanly

- Small variants pay nothing — they're inline bit patterns.
- Heavy variants get cheap-clone-by-bump semantics, matching how user
  code naturally wants to handle immutable shared data.
- One refcount per heavy Cell, not two. No `Arc<Cell>` wrapper.
- Refs hold Cells directly. No additional indirection between Ref and
  the cell.

## Cost vs the JVM

Honest accounting — Java's `ACell` references are cheaper at the
machine level because the JVM's allocator and GC are doing work we
have to do ourselves:

| Operation                               | JVM (HotSpot G1)         | Rust (`Arc<T>` + malloc) | Gap   |
|-----------------------------------------|--------------------------|--------------------------|-------|
| Copy a reference                        | ~1 ns                    | ~6 ns (mov + atomic bump) | ~6×  |
| Allocate a small object                 | ~1–2 ns (TLAB bump pointer) | ~20–50 ns (malloc)    | ~20× |
| Reclaim a dead object                   | ~5 ns amortised (GC batch) | ~5–10 ns (Arc::drop)   | ~1×  |
| Per-cell lifecycle                      | ~30–50 ns                 | ~80–150 ns               | ~3×  |

For a peer doing 500 000 cell ops/sec, the ~5% CPU overhead is real
but not blocking.

Mitigation path (apply when profiling demands):

1. **Interning common values.** A `OnceLock<Cell>` table for `nil`,
   booleans, byte flags, common keywords (`:name`, `:value`, …), and
   small integers replaces N allocations with N atomic bumps on one
   shared allocation. Probably recovers 80% of the JVM allocation
   advantage on its own — CAD3 maps tend to repeat the same keys
   constantly.
2. **Arena allocation for transaction scope.** A `bumpalo`-style arena
   gives JVM-comparable allocation throughput (~1–2 ns/alloc, bump
   pointer) for intermediate cells produced during transaction
   execution. Cells the transaction commits to state get moved out of
   the arena into long-lived storage. Doesn't change the public Cell
   API; lives behind the allocator.
3. **Epoch reclamation** (`crossbeam-epoch`) or a real Rust GC
   (`gc-arena`) only if (1) and (2) don't suffice. Substantial
   complexity; probably overkill for CAD3's workload.

These optimisations are **additive** — they don't change the type
architecture. `Arc<Inner>` for heavy variants is the right shape
regardless of whether we later allocate them from an arena or intern
common ones.

## Headline numbers

| What                                              | Size           |
|---------------------------------------------------|----------------|
| `Cell` value (any variant)                        | **16 bytes**   |
| Heap for `Cell::Nil`, `Cell::Long(42)`, etc.      | **0 bytes**    |
| Heap for `Cell::Blob(arc)` first construction     | 1 alloc, 16 B refcount header + sizeof(BlobInner) |
| Cloning a small Cell                              | 16-byte bit copy |
| Cloning a heavy Cell                              | 16-byte bit copy + 1 atomic increment |
| Dropping a small Cell                             | nothing |
| Dropping a heavy Cell                             | 1 atomic decrement; if zero, free Inner |
| `Ref` value                                       | 40 bytes (disc + max(Cell, Hash)) |

## Type architecture

```rust
use std::sync::OnceLock;
use std::sync::Arc;
use bytes::Bytes;

pub enum Cell {
    // ── Small variants (inline, no heap, no refcount) ──────────────
    Nil,
    ByteFlag(u8),                     // CAD3 0xB0..=0xBF; 0xB0/0xB1 are booleans
    Long(i64),                        // CAD3 0x10..=0x18
    Double(f64),                      // CAD3 0x1D
    Char(char),                       // CAD3 0x3C..=0x3F
    Address(u64),                     // CAD3 0xEA (extension value)

    // ── Heavy variants (Arc<Inner> — cheap clone, shared data) ─────
    BigInt(Arc<Vec<u8>>),             // CAD3 0x19
    Blob(Arc<BlobInner>),             // CAD3 0x31
    String(Arc<StringInner>),         // CAD3 0x30
    Symbol(Arc<SymbolInner>),         // CAD3 0x32
    Keyword(Arc<KeywordInner>),       // CAD3 0x33
    Vector(Arc<VectorInner>),         // CAD3 0x80
    List(Arc<ListInner>),             // CAD3 0x81
    Map(Arc<MapInner>),               // CAD3 0x82
    Set(Arc<SetInner>),               // CAD3 0x83
    Index(Arc<IndexInner>),           // CAD3 0x84
    Syntax(Arc<SyntaxInner>),         // CAD3 0x88
    Signed(Arc<SignedInner>),         // CAD3 0x90 / 0x91
    SparseRecord(Arc<SparseRecord>),  // CAD3 0xA0..=0xAF
    Code(Arc<CodedValue>),            // CAD3 0xC0..=0xCF
    DenseRecord(Arc<DenseRecord>),    // CAD3 0xD0..=0xDF
    // Other 0xEx extension values — see Open Questions.
}

pub enum Ref {
    Embedded(Cell),                   // child cell encoded inline in parent
    Branch(Hash),                     // external — Store resolves
}

pub struct BlobInner {
    data: BlobBody,
    hash: OnceLock<Hash>,             // lazy cache, populated on first value_id()
}

pub enum BlobBody {
    Leaf(Bytes),                      // ≤ 4096 bytes; may slice a parent encoding
    Tree(BlobTree),
}

pub struct BlobTree {
    total_len: u64,
    children: Box<[Ref]>,             // 2..=16 entries — Box for size containment
}

pub struct Hash(pub [u8; 32]);        // inline; not Bytes-backed
```

A note on `Box` here: it still appears, *inside* Inner types, for
variable-length child arrays (`Box<[Ref]>`). That's single-owner heap
promotion for the unique-to-this-Inner children. The outer
`Arc<BlobInner>` shares the Inner including its children array; cloning
shares everything. `Box` here is just the cheapest "this slice lives on
the heap, owned by exactly one Inner."

## Memory layout

### Cell, byte by byte

Rust lays out an enum as `discriminant + max(variant payload size)`,
respecting alignment. Largest payload is either an `i64`/`f64`/`u64`
(8 bytes) or an `Arc<…>` pointer (8 bytes); alignment is 8.

```
offset  bytes  field
──────  ─────  ─────────────────────────────────────────
  0      1     discriminant (which variant)
  1      7     padding to 8-byte alignment
  8      8     payload (largest variant is Arc<Inner> = 8 bytes)
        ──
        16    total
```

For `Cell::Nil`:

```
[ 00 ][ -- -- -- -- -- -- -- ][ -- -- -- -- -- -- -- -- ]
 disc   padding                payload (unused)
```

For `Cell::Long(42)`:

```
[ 02 ][ -- -- -- -- -- -- -- ][ 2A 00 00 00 00 00 00 00 ]
 disc   padding                i64 little-endian
```

For `Cell::Blob(arc)`:

```
[ 07 ][ -- -- -- -- -- -- -- ][ pp pp pp pp pp pp pp pp ]
 disc   padding                Arc pointer to BlobInner
                                            │
                                            ↓ heap
                                       Arc header (refcount) + BlobInner
```

> The 7 padding bytes are alignment artefact, not addressable storage
> from safe Rust. They could be reclaimed for inline data via `unsafe`
> manual tagged-union code; not worth it for v1.

### `Vec<Cell>` of 1000 booleans

```
Vec<Cell> on the heap — one contiguous array:
┌────────────┬────────────┬────────────┬─────────┬────────────┐
│ Cell #0    │ Cell #1    │ Cell #2    │   ...   │ Cell #999  │
│ ByteFlag(1)│ ByteFlag(0)│ ByteFlag(1)│         │ ByteFlag(1)│
│ 16 bytes   │ 16 bytes   │ 16 bytes   │         │ 16 bytes   │
└────────────┴────────────┴────────────┴─────────┴────────────┘
total: 16 000 bytes, one allocation, no refcounts anywhere
```

No per-cell heap allocation, no atomics. Iteration walks 16 bytes per
element, sequential memory, cache-line-aligned by 4. This is the
workload shape Cell is optimised for.

### Three parents sharing a heavy child

```
Parent 1's Cell           Parent 2's Cell           Parent 3's Cell
Cell::Vector(arc → V1)    Cell::Vector(arc → V2)    Cell::Vector(arc → V3)
       │                         │                         │
       ↓                         ↓                         ↓
  VectorInner V1            VectorInner V2            VectorInner V3
  children: Box<[Ref]> {    children: Box<[Ref]> {    children: Box<[Ref]> {
    Ref::Embedded(           Ref::Embedded(            Ref::Embedded(
      Cell::Blob(arc → B)─┐    Cell::Blob(arc → B)─┐    Cell::Blob(arc → B)─┐
    ),                    │  ),                    │  ),                    │
    …                     │  …                     │  …                     │
  }                       │  }                     │  }                     │
                          │                        │                        │
                          └────────────────────────┼────────────────────────┘
                                                   ↓
                                  shared BlobInner B (heap)
                                  ┌──────────────────────────────┐
                                  │ strong refcount = 3          │
                                  │ weak   refcount = 1          │
                                  ├──────────────────────────────┤
                                  │ BlobInner {                  │
                                  │   data: Bytes(→ buf)         │
                                  │   hash: OnceLock<Hash>       │
                                  │ }                            │
                                  └──────────────────────────────┘
```

Each parent's Vector is its own (their structure differs around the
shared child); the shared child's BlobInner has refcount = 3 and lives
once. The hash cache lives once on the shared BlobInner — first caller
to ask for `value_id()` populates it for all three.

Note there is **no `Arc<Cell>` anywhere in this picture.** The Arc is
inside `Cell::Blob`. The `Ref::Embedded(Cell)` holds the Cell by
value; cloning the Ref clones the Cell which bumps the inner Arc.
That's the entire sharing mechanism.

## Runtime discrimination

**How does code at runtime know whether a `Cell` is a `Nil`, a `Blob`,
or a `Vector`?** Two different "discriminants" are involved, and
conflating them causes confusion:

| | CAD3 tag byte | Rust enum discriminant |
|---|---|---|
| Where it lives | First byte of the canonical encoding (on the wire, on disk) | First byte of the in-memory `Cell` |
| Values | Defined by CAD3 spec — `0x00`=Nil, `0xB1`=true, `0x31`=Blob, … | Opaque small integers (0, 1, 2…) chosen by `rustc` |
| Stable across versions | Yes — wire format invariant | No — may change at every rebuild |
| Used by | Decoder / encoder | Rust pattern matching, `Drop` glue, layout |

The CAD3 tag byte is part of the encoding format. The Rust enum
discriminant is an implementation detail. They happen to play similar
roles but they are not the same bytes and must not be confused.

### How `match` dispatches

```rust
impl Cell {
    pub fn encode_into(&self, sink: &mut impl Sink) {
        match self {
            Cell::Nil               => sink.write(&[0x00]),
            Cell::ByteFlag(n)       => sink.write(&[0xB0 | n]),
            Cell::Long(v)           => encode_long(*v, sink),
            Cell::Blob(b)           => b.encode_into(sink),
            Cell::Vector(v)         => v.encode_into(sink),
            // …
        }
    }
}
```

A Rust `match` on an enum reads the discriminant byte from the cell and
either:

- **Compares + branches** when there are few variants (chain of `cmp`),
- **Uses a jump table** when there are many — the discriminant indexes
  into a table of code addresses.

Either way: **one byte load + one branch or one indexed jump**. No
vtable, no virtual function lookup, no inheritance walk. The match is
exhaustive at compile time — adding a new variant breaks every `match`
that doesn't list it. The compiler enforces "do you handle every CAD3
type" mechanically.

This is the central trade vs Java's `ACell` hierarchy. Java dispatches
through a vtable pointer in the object header (~one cache line + one
indirect call per method, plus `instanceof` for type checking). Rust
dispatches on the discriminant byte already in register-resident
memory. Typically several times faster on hot paths, and the compiler
verifies completeness.

### How `decode` dispatches

Decoding is the *other* direction — going from CAD3 wire bytes to a
`Cell`:

```rust
pub fn decode_prefix(bytes: &[u8]) -> Result<(Cell, usize), DecodeError> {
    let tag = *bytes.first().ok_or(DecodeError::Empty)?;
    match tag {
        0x00         => Ok((Cell::Nil, 1)),
        0x10..=0x18  => decode_long(tag, &bytes[1..]),
        0x19         => decode_bigint(&bytes[1..]),
        0x1D         => decode_double(&bytes[1..]),
        0x30         => decode_string(&bytes[1..]),
        0x31         => decode_blob(&bytes[1..]),
        0x32         => decode_symbol(&bytes[1..]),
        0x33         => decode_keyword(&bytes[1..]),
        0x3C..=0x3F  => decode_char(tag, &bytes[1..]),
        0x80         => decode_vector(&bytes[1..]),
        0x81         => decode_list(&bytes[1..]),
        // …
        0xB0..=0xBF  => Ok((Cell::ByteFlag(tag & 0x0F), 1)),
        // …
        0xFF         => Err(DecodeError::IllegalTag),
        _            => Err(DecodeError::UnknownTag(tag)),
    }
}
```

So:

- **Encode:** Rust `match` on the enum discriminant produces CAD3 tag bytes.
- **Decode:** Rust `match` on the CAD3 tag byte constructs the Rust
  enum variant.

The two discriminants are bridged exactly twice in the codebase — at
encode and at decode. Everywhere else, code works with the Rust enum
shape.

## Hash

```rust
pub struct Hash(pub [u8; 32]);
```

Inline 32 bytes. Equality is a single `[u8; 32]` compare, typically
SIMD. No `Bytes`-backing — the structural-sharing case applies to data
(KBs of Blob bytes, child Cells), not to fixed-size fingerprints.

## Refs

```rust
pub enum Ref {
    Embedded(Cell),
    Branch(Hash),
}
```

Memory: 1-byte discriminant + 7 padding + max(16-byte Cell, 32-byte
Hash) = **40 bytes per Ref**. A VectorTree with 16 children needs
`Box<[Ref; 16]>` = 640 bytes for the children array, plus the small
header.

`Embedded(Cell)` is used when the child cell's encoding is inlined in
the parent's encoding (CAD3 says embedded children ≤ 140 bytes).
Holding `Cell` by value here works because cloning a Cell is cheap
(small variants bit-copy; heavy variants bump the inner Arc).

`Branch(Hash)` is used when the child is external (its encoding lives
in storage; the parent's encoding only contains the 32-byte value-ID).
Resolution — turning a `Hash` into a `Cell` — is the Store's job, not
`Ref`'s; we expose it via a `Store` trait passed in by the host.

## Three layers of sharing

| Java does this via                       | CAD3 use case                                                  | Rust primitive               |
|------------------------------------------|----------------------------------------------------------------|------------------------------|
| `ACell` heap object reference            | Two parents pointing at the same child cell                    | `Arc<Inner>` inside the variant payload |
| `byte[]` + offset/length                 | Blob/String chunks sliced from a larger encoding buffer        | `bytes::Bytes`               |
| cached `cachedHash` field                | Pay-once value ID per heavy cell                               | `OnceLock<Hash>` on each Inner |

Drop any one and you have broken structural sharing somewhere.

## Identity, equality, hashing

- `PartialEq` on `Cell`:
  - Small variants: bitwise compare.
  - Heavy variants: `Arc::ptr_eq` fast-path (same allocation → equal),
    then structural compare via the Inner if pointers differ.
- For semantic equality across separately-constructed equivalent cells,
  compare value-IDs: `a.value_id() == b.value_id()` — single 32-byte
  compare, sufficient by SHA3-256 collision assumption.
- Identity (same heap allocation): `Arc::ptr_eq` on the inner Arc.
  Useful in storage / intern caches; not normally in user code.

## Decoding shares with the input buffer

```rust
pub fn decode(buf: Bytes) -> Result<Cell, DecodeError>;
```

Decoding takes `Bytes` (not `&[u8]`). Every Blob/String leaf extracted
from the input is `buf.slice(range)` — zero copy. Drop the root `Cell`
and the underlying allocation is freed.

`Hash` values inside `Ref::Branch` are copied (32 bytes inline) — the
parent buffer is freed once decoding is done.

## Thread safety

`bytes::Bytes: Send + Sync`. `Arc<T>: Send + Sync` when
`T: Send + Sync`. `OnceLock<T>: Send + Sync` when `T: Send + Sync`. All
Inner types contain only `Send + Sync` data, so `Cell: Send + Sync`
and `Ref: Send + Sync` fall out automatically.

Compile-time tripwire (already in `src/lib.rs`):

```rust
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Cell>();
    assert_send_sync::<Ref>();
    assert_send_sync::<Hash>();
};
```

The `const _: fn() = || { … }` form binds a never-called closure into
a `const` slot. The compiler still type-checks the body, so the trait
bounds are verified at build time. If a future variant breaks
`Send + Sync` the build fails.

## Path-copying example

Updating index 5 of a 1024-element Vector. Vector is a tree of
factor-16 nodes:

```
                         V (top Cell::Vector)
                       Arc → VectorInner
                       │
                       ↓ heap
                  VectorInner {
                    children: Box<[Ref; 16]> {
                      Ref::Embedded(Cell::Vector(arc)) ──→ subtree A (64 elems)
                      Ref::Embedded(Cell::Vector(arc)) ──→ subtree B
                      …
                      Ref::Embedded(Cell::Vector(arc)) ──→ subtree P
                    }
                  }

V.assoc(5, new_value) produces V':

V' is a new Cell::Vector wrapping a new VectorInner.
V'.children[1..16]   shares the same inner-Arc Cells as V.children[1..16]   (15 atomic bumps)
V'.children[0]       is a new Cell::Vector wrapping a new subtree A'
A'.children[1..16]   shares with the original A.children[1..16]              (15 atomic bumps)
… and so on down to the leaf containing index 5.

Total new allocations: one new VectorInner per tree level + one new leaf — ≤ 4 heap allocs for 1024 elements.
Total bytes copied: zero — every shared subtree is an Arc bump on the inner Arc inside Cell::Vector.
```

The Arc bumps live inside the cloned `Cell::Vector(arc)` values stored
in `Ref::Embedded`. V and V' coexist; either can be freed
independently; sub-trees referenced by both stay alive as long as
either does.

## Trait surface

Java's abstract base classes (`ABlob`, `ABlobLike`, `ACountable`,
`IAssociative`, `ASequence`) translate to **traits** — purely
behavioural interfaces:

```rust
pub trait BlobLike {
    fn byte_len(&self) -> usize;
    fn get_byte(&self, i: usize) -> Option<u8>;
    fn as_bytes(&self) -> Bytes;
}

impl BlobLike for BlobInner { /* … */ }
impl BlobLike for StringInner { /* … */ }
impl BlobLike for SymbolInner { /* … */ }
impl BlobLike for KeywordInner { /* … */ }
```

To "ask a Cell if it's BlobLike":

```rust
impl Cell {
    pub fn as_blob_like(&self) -> Option<&dyn BlobLike> {
        match self {
            Cell::Blob(b)    => Some(&**b),    // &**b: deref Arc, then take &
            Cell::String(s)  => Some(&**s),
            Cell::Symbol(s)  => Some(&**s),
            Cell::Keyword(k) => Some(&**k),
            _ => None,
        }
    }
}
```

`&**arc` is the Rust idiom for "take `&Arc<T>`, deref to `T`, take
`&T`." From `&T` to `&dyn BlobLike` is automatic when `T: BlobLike`.

There is no inheritance to walk; the relationship is flat and visible.

## What this design deliberately does not address

- **Storage.** Loading branch refs from Etch / an on-disk store is the
  job of a `Store` trait passed in by the host application.
  `Ref::Branch(Hash)` exposes the hash; resolution is elsewhere.
- **Network acquisition.** Outside the crate.
- **CVM semantics.** Cells encode CVM values; they do not execute them.
- **Weak references for cache eviction.** A long-running peer will
  want to evict cold cells. That belongs in the store layer.
- **Arena allocation.** A future addition behind the Arc — same public
  Cell API. See *Cost vs the JVM* above.

## Open questions

1. **Extension values (0xEx) other than Address.** A naïve payload of
   `(u8, u64)` doesn't fit in our 8-byte enum slot. Options:
   - Bit-pack: `Extension(u64)` with top 4 bits as code, low 60 as value.
     Costs 3 bits of VLQ range (max 2⁶⁰); fine.
   - Box: `Extension(Box<(u8, u64)>)` — keeps enum compact, heap-allocs
     per extension.
   Lean towards bit-pack.
2. **`Ref::Branch` lazy loader vs Store lookup.** Current proposal:
   `Branch(Hash)` only; resolution via `Store::get(hash)`. Alternative:
   `Branch(Hash, OnceLock<Cell>)` self-caches. Current is cleaner;
   Store does its own caching.
3. **Hash cache: only on heavy Inners?** Yes. Small variants compute on
   demand. Confirmed.
4. **Interning policy** — which common values get a static `Cell` slot?
   Candidates: empty Blob, empty String, common keywords
   (`:name`, `:value`, …), small integers (0..=255), ASCII characters.
   This is the **highest-leverage easy win** against the JVM allocation
   advantage.
5. **Arena allocation** for transaction-scope cells. Future addition,
   doesn't change the type. Trigger: profiling shows transaction
   execution dominated by alloc churn.

## Migration from the current scaffold

The crate today has `Cell` as a `Clone`-but-not-`Copy` enum with `Nil`
and `ByteFlag`. Migration steps:

1. ~~Drop `Copy`, keep `Clone`.~~ **Done** (commit `b5d8a44`).
2. ~~Add `Send + Sync` const tripwire.~~ **Done** (commit `b5d8a44`).
3. Add `Cell::Long(i64)` — the first variant that exercises the
   full 16-byte enum slot and the encode/decode dispatch shape.
4. Add `bytes = "1"` dependency and `Cell::Blob(Arc<BlobInner>)` — the
   first heavy variant. First place `Arc<Inner>`, `Bytes`, and
   `OnceLock<Hash>` actually appear.
5. Pin `std::mem::size_of::<Cell>() == 16` in tests once `Long` lands.
6. Add `Ref` enum once we have something to embed (Vector will be the
   forcing function).
7. Interning of common Cells (start with `nil` and the 16 byte flags)
   once we have enough surface to feel the alloc churn.
