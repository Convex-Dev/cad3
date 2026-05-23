# cad3

[![CI](https://github.com/Convex-Dev/cad3/actions/workflows/ci.yml/badge.svg)](https://github.com/Convex-Dev/cad3/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)

Rust implementation of [Convex CAD3](https://docs.convex.world/cad/003_encoding/) — the
canonical wire and storage encoding for Convex lattice data values.

> **Status:** early scaffold. Supports `nil`, byte flags, and the CVM booleans. New CAD3
> types land here as they're implemented. The Java reference implementation lives at
> [`Convex-Dev/convex`](https://github.com/Convex-Dev/convex) under
> `convex-core/src/main/java/convex/core/data/`.

## What's CAD3?

CAD3 is a compact, self-describing, canonical binary encoding for immutable data values.
Every value has exactly one valid byte representation, and the SHA3-256 hash of that
encoding is the value's stable, decentralised identifier (the "value ID"). Larger values
form Merkle DAGs via embedded child encodings or external value-ID references, enabling
structural sharing, partial transmission, and arbitrary-size data structures inside
fixed-size buffers.

See the [CAD3 specification](https://docs.convex.world/cad/003_encoding/) for full details.

## Implemented

| Tag           | Type        | Notes                                       |
| ------------- | ----------- | ------------------------------------------- |
| `0x00`        | `nil`       | single-byte encoding                        |
| `0xB0`–`0xBF` | Byte flags  | `0xB0` = CVM `false`, `0xB1` = CVM `true`   |

Roadmap (in rough order): integers, doubles, blobs, strings, symbols, keywords,
characters, vectors, lists, maps, sets, refs, signed data, records.

## Usage

```toml
[dependencies]
cad3 = "0.0.1"
```

```rust
use cad3::Cell;

// Encoding
assert_eq!(Cell::Nil.encoding(),   vec![0x00]);
assert_eq!(Cell::FALSE.encoding(), vec![0xB0]);
assert_eq!(Cell::TRUE.encoding(),  vec![0xB1]);

// Round trip
let cell = Cell::decode(&Cell::TRUE.encoding()).unwrap();
assert_eq!(cell, Cell::TRUE);

// Value ID = SHA3-256 of the canonical encoding
let id = Cell::TRUE.value_id();
println!("value id of true = {id}");
```

## Development

Requires Rust 1.75 or later.

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
