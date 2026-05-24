# cad3

[![CI](https://github.com/Convex-Dev/cad3/actions/workflows/ci.yml/badge.svg)](https://github.com/Convex-Dev/cad3/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)

Rust implementation of [Convex CAD3](https://docs.convex.world/cad/003_encoding/) — the
canonical wire and storage encoding for Convex lattice data values.

> **Status:** early. The small primitive variants are landed; collections,
> blobs/strings, and the rest are next. The Java reference implementation
> lives at [`Convex-Dev/convex`](https://github.com/Convex-Dev/convex) under
> `convex-core/src/main/java/convex/core/data/`. Architectural design lives
> in [`docs/CELL_DESIGN.md`](docs/CELL_DESIGN.md).

## What's CAD3?

CAD3 is a compact, self-describing, canonical binary encoding for immutable data values.
Every value has exactly one valid byte representation, and the SHA3-256 hash of that
encoding is the value's stable, decentralised identifier (the "value ID"). Larger values
form Merkle DAGs via embedded child encodings or external value-ID references, enabling
structural sharing, partial transmission, and arbitrary-size data structures inside
fixed-size buffers.

See the [CAD3 specification](https://docs.convex.world/cad/003_encoding/) for full details.

## Implemented

| Tag(s)        | Type        | Notes                                       |
| ------------- | ----------- | ------------------------------------------- |
| `0x00`        | `nil`       | single-byte encoding                        |
| `0x10`–`0x18` | Long        | signed `i64`, minimal-length two's complement |
| `0x1D`        | Double      | IEEE 754, canonical NaN                     |
| `0x3C`–`0x3E` | Char        | Unicode scalar, minimal-length              |
| `0xB0`–`0xBF` | Byte flags  | `0xB0` = CVM `false`, `0xB1` = CVM `true`   |
| `0xEA`        | Address     | account index, VLQ payload                  |

Roadmap (in rough order): BigInt, blobs, strings, symbols, keywords,
refs, vectors, lists, maps, sets, indexes, signed data, records.

## Usage

```toml
[dependencies]
cad3 = "0.0.1"
```

```rust
use cad3::Cell;

// Encoding — one canonical byte sequence per value
assert_eq!(Cell::Nil.encoding(),          vec![0x00]);
assert_eq!(Cell::FALSE.encoding(),        vec![0xB0]);
assert_eq!(Cell::Long(19).encoding(),     vec![0x11, 0x13]);
assert_eq!(Cell::Long(-1).encoding(),     vec![0x11, 0xFF]);
assert_eq!(Cell::Char('A').encoding(),    vec![0x3C, 0x41]);
assert_eq!(Cell::Address(128).encoding(), vec![0xEA, 0x81, 0x00]);

// Round trip
let cell = Cell::decode(&Cell::Long(42).encoding()).unwrap();
assert_eq!(cell, Cell::Long(42));

// Value ID = SHA3-256 of the canonical encoding
let id = Cell::Long(42).value_id();
println!("value id = {id}");
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
