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

Secondary goal: **a fresh `Cell` value pays no overhead it doesn't need.**
A `Cell::Long(42)` constructed as a function-local intermediate should
not heap-allocate and should not increment a refcount. Refcounting is for
sharing; non-shared values stay on the stack.

## The shape — load-bearing insight

A `Cell` *is* the value. Sharing happens **only** when one cell appears
as a child inside another, and that one place is `Ref`:

```rust
pub enum Cell {                       // 16-byte enum, inline value type
    Nil,
    ByteFlag(u8),
    Long(i64),
    // … small variants inline …
    Blob(Box<BlobInner>),             // heavy variants Box'd for size
    Vector(Box<VectorInner>),
    // …
}

pub enum Ref {                        // the ONLY place an Arc lives
    Embedded(Arc<Cell>),              // child cell shared with parent
    Branch(Hash),                     // external — Store resolves
}
```

Everywhere a "child cell pointer" would naturally appear (Vector
elements, Map entries, Syntax wrapped value, Signed wrapped value,
Refs in tree nodes) you find `Ref`. Everywhere else, a `Cell` is just a
value.

## Headline numbers

| What                                              | Size           |
|---------------------------------------------------|----------------|
| `Cell` value (any variant)                        | **16 bytes**   |
| Heap for `Cell::Nil`, `Cell::Long(42)`, etc.      | **0 bytes**    |
| Heap for `Cell::Blob(box_blob_inner)`             | 1 alloc, sizeof(BlobInner) |
| Cloning a small Cell                              | 16-byte bit copy |
| Cloning a heavy Cell                              | bit copy + Box deep-clone of inner (discouraged — see below) |
| Cloning an `Arc<Cell>`                            | atomic refcount bump |
| `Ref::Embedded(arc)`                              | 8-byte Arc pointer + discriminant slot ≈ 40 bytes  |
| `Ref::Branch(hash)`                               | 32-byte hash + discriminant slot ≈ 40 bytes |

## Box vs Arc — the distinction that matters

| | `Box<T>` | `Arc<T>` |
|---|---|---|
| Purpose | Heap-promote `T` to keep an enum compact | Shared ownership of `T` |
| Wire size of the pointer | 8 bytes | 8 bytes |
| Heap overhead per allocation | 0 (just `T`) | 16 bytes refcount header + `T` |
| Clone | Deep-copies `T` | Atomic refcount bump |
| Used in our design | Inside `Cell` variants — single-owner size containment | Inside `Ref::Embedded` — multi-owner sharing |

`Box` is "this lives behind a pointer so the enum stays small." `Arc` is
"multiple things want to point at this." Our Cell variants use `Box` —
each `Cell::Blob` owns its `BlobInner` outright. `Ref` uses `Arc` because
that is precisely where two parents need to point at the same child.

## Type architecture

```rust
use std::sync::OnceLock;
use std::sync::Arc;
use bytes::Bytes;

pub enum Cell {
    // ── Small variants (payload fits inline, no heap) ──────────────
    Nil,
    ByteFlag(u8),                     // CAD3 0xB0..=0xBF; 0xB0/0xB1 are booleans
    Long(i64),                        // CAD3 0x10..=0x18
    Double(f64),                      // CAD3 0x1D
    Char(char),                       // CAD3 0x3C..=0x3F
    Address(u64),                     // CAD3 0xEA (extension value)

    // ── Heavy variants (Box'd; cache lives inside the Inner) ───────
    BigInt(Box<Vec<u8>>),             // CAD3 0x19
    Blob(Box<BlobInner>),             // CAD3 0x31
    String(Box<StringInner>),         // CAD3 0x30
    Symbol(Box<SymbolInner>),         // CAD3 0x32
    Keyword(Box<KeywordInner>),       // CAD3 0x33
    Vector(Box<VectorInner>),         // CAD3 0x80
    List(Box<ListInner>),             // CAD3 0x81
    Map(Box<MapInner>),               // CAD3 0x82
    Set(Box<SetInner>),               // CAD3 0x83
    Index(Box<IndexInner>),           // CAD3 0x84
    Syntax(Box<SyntaxInner>),         // CAD3 0x88
    Signed(Box<SignedInner>),         // CAD3 0x90 / 0x91
    SparseRecord(Box<SparseRecord>),  // CAD3 0xA0..=0xAF
    Code(Box<CodedValue>),            // CAD3 0xC0..=0xCF
    DenseRecord(Box<DenseRecord>),    // CAD3 0xD0..=0xDF
    // Other 0xEx extension values — packed; see Open Questions.
}

pub enum Ref {
    Embedded(Arc<Cell>),
    Branch(Hash),
}

pub struct BlobInner {
    data: BlobBody,                   // Leaf(Bytes) | Tree(VectorTree-like)
    hash: OnceLock<Hash>,             // lazy cache, pays only on heavy variants
}

pub enum BlobBody {
    Leaf(Bytes),                      // ≤ 4096 bytes; may slice a parent encoding
    Tree(BlobTree),
}

pub struct BlobTree {
    total_len: u64,
    children: Box<[Ref]>,             // 2..=16 entries
}

pub struct Hash(pub [u8; 32]);        // inline; no Bytes-backing
```

The cache (`OnceLock<Hash>`) lives **only on the Inner types** that
actually pay for it. Small variants don't have a cache — hashing 1–9
bytes is fast enough that caching costs more than recomputing.

## Memory layout

### Cell, byte by byte

Rust lays out an enum as `discriminant + max(variant payload size)`,
aligned. With ~20 variants the discriminant is one byte; with all
payloads being either ≤ 8 bytes or `Box<...>` (also 8 bytes), the
payload slot is 8 bytes; 8-byte alignment forces a full word for the
discriminant.

```
offset  bytes  field
──────  ─────  ─────────────────────────────────────────
  0      1     discriminant (which variant; e.g. 0x00 = Nil, 0x07 = Blob)
  1      7     padding to 8-byte alignment
  8      8     payload (largest variant is Box<...> = 8 bytes)
        ──
        16    total
```

For `Cell::Nil`:

```
[ 00 ][ -- -- -- -- -- -- -- ][ -- -- -- -- -- -- -- -- ]
 disc   padding                payload (unused for Nil)
```

For `Cell::ByteFlag(1)` (CVM `true`):

```
[ 01 ][ -- -- -- -- -- -- -- ][ 01 -- -- -- -- -- -- -- ]
 disc   padding                u8 in low byte
```

For `Cell::Long(42)`:

```
[ 02 ][ -- -- -- -- -- -- -- ][ 2A 00 00 00 00 00 00 00 ]
 disc   padding                i64 little-endian
```

For `Cell::Blob(boxed_inner)`:

```
[ 07 ][ -- -- -- -- -- -- -- ][ pp pp pp pp pp pp pp pp ]
 disc   padding                Box pointer to BlobInner
                                            │
                                            ↓ heap
```

> **Note:** the discriminant byte values are illustrative — the Rust
> compiler picks them, and they can change at any rebuild. They are not
> the same thing as CAD3 tag bytes. See *Runtime discrimination* below.

### A `Vec<Cell>` of 1000 booleans

```
Vec<Cell> on the heap — one contiguous array:
┌────────────┬────────────┬────────────┬─────────┬────────────┐
│ Cell #0    │ Cell #1    │ Cell #2    │   ...   │ Cell #999  │
│ ByteFlag(1)│ ByteFlag(0)│ ByteFlag(1)│         │ ByteFlag(1)│
│ 16 bytes   │ 16 bytes   │ 16 bytes   │         │ 16 bytes   │
└────────────┴────────────┴────────────┴─────────┴────────────┘
total: 16 000 bytes, one allocation
```

No per-cell heap allocation, no refcounts, no pointers. Iterating
through this Vec walks 16 bytes per element, sequential memory,
cache-line-aligned by 4. Compiles to an unrolled SIMD loop in tight
code. This is the workload-shape Cell is optimised for.

### Three parents sharing a heavy child via `Ref::Embedded`

```
Parent Cell #1                  Parent Cell #2                  Parent Cell #3
  Cell::Vector(box_vinner_a)      Cell::Vector(box_vinner_b)      Cell::Vector(box_vinner_c)
       │                               │                               │
       ↓ heap                          ↓ heap                          ↓ heap
   VectorInner A                   VectorInner B                   VectorInner C
   children: Box<[Ref]> {          children: Box<[Ref]> {          children: Box<[Ref]> {
     Ref::Embedded(arc) ─┐           Ref::Embedded(arc) ─┐           Ref::Embedded(arc) ─┐
     …                   │           …                   │           …                   │
   }                     │         }                     │         }                     │
                         │                               │                               │
                         └───────────────────────────────┼───────────────────────────────┘
                                                         ↓
                              shared heap allocation X (Arc<Cell>'s target)
                              ┌──────────────────────────────────┐
                              │ strong refcount = 3              │ 16 bytes
                              │ weak   refcount = 1              │
                              ├──────────────────────────────────┤
                              │ Cell::Blob(Box<BlobInner>)       │ 16 bytes
                              └──────────────────────────────────┘
                                                         │
                                                         ↓
                                                 BlobInner heap alloc
                                                 ┌──────────────────────┐
                                                 │ data: Bytes(→ buf)   │
                                                 │ hash: OnceLock<Hash> │
                                                 └──────────────────────┘
```

The shared child is `Arc<Cell>` because `Ref::Embedded` is the sharing
point. Cloning a Ref bumps that Arc's refcount; the inner Cell — and
the BlobInner it owns — is shared across all three parents. The hash
cache lives once, on the shared BlobInner.

Each parent has its own VectorInner (it's *their* vector); only the
shared child gets the Arc treatment.

## Runtime discrimination

This is what you asked about specifically. **How does code at runtime
know whether a `Cell` is a `Nil`, a `Blob`, or a `Vector`?** Two
different "discriminants" are involved, and conflating them causes
confusion:

| | CAD3 tag byte | Rust enum discriminant |
|---|---|---|
| Where it lives | First byte of the canonical encoding (on the wire, on disk) | First byte of the in-memory `Cell` |
| Values | Defined by CAD3 spec — `0x00`=Nil, `0xB1`=true, `0x31`=Blob, … | Opaque small integers (0, 1, 2…) chosen by `rustc` |
| Stable across versions | Yes — wire format invariant | No — may change at every rebuild |
| Used by | Decoder / encoder | Rust pattern matching, `Drop` glue, layout |

The CAD3 tag byte is part of the encoding format. The Rust enum
discriminant is an implementation detail of how the enum is laid out in
memory. They happen to play similar roles but they are not the same
bytes and must not be confused.

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

A Rust `match` on an enum reads the discriminant byte from the cell, and
either:

- **Compares + branches** when there are few variants (the compiler
  emits a chain of `cmp` instructions), or
- **Uses a jump table** when there are many variants — the discriminant
  indexes into a table of code addresses (`switch` in LLVM IR, `jmpq *(%rax,%rcx,8)`
  in x86_64 asm).

Either way: **one byte load + one branch or one indexed jump**. No
vtable, no virtual function lookup, no inheritance walk. The match
itself is exhaustive at compile time — if you add a new variant to
`Cell`, every `match` that doesn't handle it fails to compile. The
compiler enforces "do you handle every CAD3 type" mechanically.

This is the central trade vs Java's `ACell` hierarchy. Java dispatches
through a vtable pointer in the object header (~one cache line + one
indirect call per method invocation, plus `instanceof` for type
checking). Rust dispatches on the discriminant byte already in
register-resident memory (one compare or one indexed jump). The Rust
path is typically several times faster on hot paths, and crucially the
compiler verifies completeness.

### How `decode` dispatches

Decoding is the *other* direction — going from CAD3 wire bytes to a
`Cell`:

```rust
pub fn decode_prefix(bytes: &[u8]) -> Result<(Cell, usize), DecodeError> {
    let tag = *bytes.first().ok_or(DecodeError::Empty)?;
    match tag {
        0x00 => Ok((Cell::Nil, 1)),
        0x10..=0x18 => decode_long(tag, &bytes[1..]),
        0x19 => decode_bigint(&bytes[1..]),
        0x1D => decode_double(&bytes[1..]),
        0x20 => Err(DecodeError::RefIsNotACell), // 0x20 is a Ref payload, not a cell
        0x30 => decode_string(&bytes[1..]),
        0x31 => decode_blob(&bytes[1..]),
        // …
        0xB0..=0xBF => Ok((Cell::ByteFlag(tag & 0x0F), 1)),
        // …
        0xFF => Err(DecodeError::IllegalTag),
        other => Err(DecodeError::UnknownTag(other)),
    }
}
```

The CAD3 tag byte drives this dispatch. Output variants are constructed
explicitly. So:

- **Encode:** Rust `match` on the enum discriminant produces CAD3 tag bytes.
- **Decode:** Rust `match` on the CAD3 tag byte constructs the Rust enum
  variant.

The two discriminants are bridged exactly twice in the codebase — at
encode and at decode. Everywhere else, code works with the Rust enum
shape.

### Cost of a `match`

A `match cell { … }` in release mode compiles to between 5 and 20 x86_64
instructions for the dispatch itself, depending on whether the compiler
chose a jump table or a compare chain. Cells already in L1 cache match
in ~1 ns. The variant body (the `=>` arm) is then whatever logic it
needs.

A pseudo-disassembly (intel syntax, simplified):

```
movzx eax, byte ptr [rdi]    ; load discriminant from cell pointer
cmp   eax, 0x07              ; is it Blob?
je    .blob_arm
cmp   eax, 0x00              ; is it Nil?
je    .nil_arm
…
```

Or with a jump table:

```
movzx eax, byte ptr [rdi]
jmp   qword ptr [.jump_table + rax * 8]
```

Either is essentially free on modern CPUs.

## Hash

```rust
pub struct Hash(pub [u8; 32]);
```

Inline 32 bytes. Equality is a single `[u8; 32]` compare, typically SIMD.
Identical-size to a `Bytes` handle but no indirection. See the earlier
discussion — the structural-sharing case applies to *data* (KBs of
Blob bytes, child Cells), not to fixed-size fingerprints. Bytes-backing
a `Hash` saves at most 32 bytes per extraction at the cost of an
allocation per fresh hash; net wash.

## Refs

```rust
pub enum Ref {
    Embedded(Arc<Cell>),
    Branch(Hash),
}
```

Memory: 1-byte discriminant + 7 padding + max(8-byte Arc pointer,
32-byte Hash) = **40 bytes per Ref**. A VectorTree with 16 children
needs `Box<[Ref; 16]>` = 640 bytes for the children array, plus the
small header. That's unavoidable cost of a non-leaf node carrying 16
typed child pointers + lazy resolution options.

`Embedded` is used when the child cell's encoding is inlined in the
parent's encoding (CAD3 says embedded children ≤ 140 bytes). The
`Arc<Cell>` here serves two purposes: it's the child cell, *and* it
permits multiple parents to share that cell.

`Branch` is used when the child is external (its encoding lives in
storage; the parent's encoding only contains the 32-byte value-ID).
Resolution — turning a `Hash` into a `Cell` — is the Store's job, not
`Ref`'s; we expose it via a `Store` trait passed in by the host.

## Three layers of sharing

| Java does this via                       | CAD3 use case                                                  | Rust primitive               |
|------------------------------------------|----------------------------------------------------------------|------------------------------|
| `ACell` heap object reference            | Two parents pointing at the same child cell                    | `Arc<Cell>` inside `Ref::Embedded` |
| `byte[]` + offset/length                 | Blob/String chunks sliced from a larger encoding buffer        | `bytes::Bytes`               |
| cached `cachedHash` field                | Pay-once value ID per heavy cell                               | `OnceLock<Hash>` on each `*Inner`  |

Drop any one and you have broken structural sharing somewhere.

## Identity, equality, hashing

- `PartialEq` on `Cell`:
  - Small variants: bitwise compare (it's a `Copy` payload — pure value).
  - Heavy variants: structural compare via the Box'd Inner. Fast-path
    on `value_id()` equality (one `[u8; 32]` compare) if both have
    populated hash caches.
- For semantic equality across separately-constructed equivalent cells,
  compare value-IDs: `a.value_id() == b.value_id()` — single 32-byte
  compare, sufficient by the SHA3-256 collision assumption.
- Identity (same allocation) for a shared `Arc<Cell>`: `Arc::ptr_eq`.
  Useful in storage / intern caches; not normally in user code.

## Decoding shares with the input buffer

```rust
pub fn decode(buf: Bytes) -> Result<Cell, DecodeError>;
```

Decoding takes `Bytes` (not `&[u8]`). Every Blob/String leaf extracted
from the input is `buf.slice(range)` — zero copy. Drop the root `Cell`
and the underlying allocation is freed.

`Hash` values inside `Ref::Branch` are copied (32 bytes inline) — the
parent buffer can be freed once decoding is done. If profiling ever
shows this as hot we can switch `Hash` to `Bytes`-backed; it's a local
change.

## Thread safety

`bytes::Bytes: Send + Sync`. `Arc<T>: Send + Sync` when `T: Send + Sync`.
`OnceLock<T>: Send + Sync` when `T: Send + Sync`. Box of `Send + Sync`
is `Send + Sync`. All `*Inner` types contain only `Send + Sync` data,
so `Cell: Send + Sync` and `Ref: Send + Sync` fall out automatically.

Compile-time tripwire:

```rust
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Cell>();
    assert_send_sync::<Ref>();
    assert_send_sync::<Hash>();
};
```

The `const _: fn() = || { … }` form binds a never-called closure into a
`const` slot. The compiler still type-checks the body, so the trait
bounds are verified at build time. If any future variant breaks
`Send + Sync` (e.g. someone adds an `Rc` field), the build fails.

## Path-copying example

Updating index 5 of a 1024-element Vector. Vector is a tree of
factor-16 nodes; this is the canonical structural-sharing win:

```
                         V (top Cell::Vector)
                       Box<VectorInner>
                       │
                       ↓ heap
                  VectorInner {
                    children: Box<[Ref; 16]> {
                      Ref::Embedded(Arc<Cell>) ──→ subtree A (64 elems)
                      Ref::Embedded(Arc<Cell>) ──→ subtree B
                      …
                      Ref::Embedded(Arc<Cell>) ──→ subtree P
                    }
                  }

V.assoc(5, new_value) produces V':

V' is a new Cell::Vector wrapping a new VectorInner.
V'.children[1..16]   shares the same Arc<Cell>s as V.children[1..16]   (15 atomic bumps)
V'.children[0]       is a new Arc<Cell> wrapping a new subtree A'
A'.children[1..16]   shares the same Arc<Cell>s as A.children[1..16]   (15 atomic bumps)
… and so on down to the leaf containing index 5.

Total new allocations: one new VectorInner per tree level + one new
leaf — ≤ 4 heap allocs for 1024 elements.
Total bytes copied: zero — every shared subtree is an Arc bump.
```

The Arc bumps live in `Ref::Embedded(Arc<Cell>)` — exactly where
sharing happens. V and V' coexist; either can be freed independently;
sub-trees referenced by both stay alive as long as either does.

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
            Cell::Blob(b)    => Some(&**b),    // &**b: deref Box, then &
            Cell::String(s)  => Some(&**s),
            Cell::Symbol(s)  => Some(&**s),
            Cell::Keyword(k) => Some(&**k),
            _ => None,
        }
    }
}
```

`&**b` is the Rust idiom for "take a `&Box<T>`, deref the Box to get
`T`, then take a reference: `&T`." From `&T` to `&dyn BlobLike` is an
automatic coercion since `T: BlobLike`.

Pattern matching adds a Cell variant to the cross-traits dispatch
explicitly. There is no inheritance to walk; the relationship is
flat and visible.

## What this design deliberately does not address

- **Storage.** Loading branch refs from Etch / an on-disk store is the
  job of a `Store` trait passed in by the host application.
  `Ref::Branch(Hash)` exposes the hash; resolution is elsewhere.
- **Network acquisition.** Outside the crate.
- **CVM semantics.** Cells encode CVM values; they do not execute them.
- **Weak references for cache eviction.** A long-running peer will want
  to evict cold cells. That belongs in the store layer.
- **Interning of common cells.** A `static EMPTY_BLOB: OnceLock<Arc<Cell>>`
  table for frequently-reused values is an optional optimisation.
  Small variants don't need interning — copying a 16-byte enum is
  already free.

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
   `Branch(Hash, OnceLock<Arc<Cell>>)` self-caches the resolved cell.
   Current is cleaner; Store does its own caching.
3. **Hash cache: only on heavy Inners?** Yes. Small variants compute on
   demand. Confirm.
4. **Should `Cell` impl `Clone`?** Reluctantly yes (Rust convention),
   but document that for heavy variants `Arc::clone(&Arc::new(cell))`
   is the right pattern when sharing is intended.
5. **Interning policy** — which common values get static `Arc<Cell>`
   slots? Candidates: empty Blob, empty String, common keywords
   (`:name`, `:value`, …), empty Vector/Map/Set.

## Migration from the current scaffold

The crate today has `Cell` as a `Copy` enum with `Nil` and `ByteFlag`.
To land this design:

1. Add `bytes = "1"` dependency (only needed when Blob lands; can wait).
2. Change `Cell` to `#[derive(Clone)]` (drop `Copy`). Today's two
   variants are still bit patterns; nothing semantic changes.
3. Add `Cell::Long(i64)` as the next variant — exercises the
   16-byte enum slot and the encode/decode dispatch shape.
4. Add `Ref` enum once we have something to embed (Vector will be the
   forcing function).
5. Introduce `*Inner` types and `OnceLock<Hash>` caches when Blob lands
   — first place caching pays off.
6. Add the `Send + Sync` const tripwire.

The diff to step 2 is one-line (`#[derive(Clone)]` instead of
`#[derive(Clone, Copy)]`) and doesn't change any byte. Steps 3–6 land
one variant at a time.
