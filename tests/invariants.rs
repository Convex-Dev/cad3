//! Generic cross-cutting invariants that every [`Cell`](cad3::Cell) must
//! satisfy. Runs the corpus from `common::corpus()` through
//! `common::assert_cell_invariants`. Mirrors the JVM `GenTestFormat`
//! pattern — add a variant to the corpus and it inherits the full suite.

mod common;

use cad3::{Cell, Hash};
use common::{assert_cell_invariants, corpus};

#[test]
fn corpus_covers_every_implemented_variant() {
    // Sanity: any new variant added to Cell must show up in at least one
    // corpus entry. We assert presence by discriminant — if a future
    // variant doesn't appear in the corpus, this test fails loudly.
    use std::collections::HashSet;
    use std::mem::discriminant;
    let kinds: HashSet<_> = corpus().iter().map(discriminant).collect();
    // Currently 8 variants: Nil, ByteFlag, Long, Double, Char, Address,
    // Blob, String. Bump this number when adding new variants AND update
    // the corpus.
    assert_eq!(
        kinds.len(),
        8,
        "corpus missing a Cell variant (or has too many) — update common::corpus"
    );
}

#[test]
fn every_corpus_cell_satisfies_all_invariants() {
    for cell in corpus() {
        assert_cell_invariants(&cell);
    }
}

#[test]
fn value_id_uniqueness_across_corpus() {
    // Pragmatic check: every cell in our corpus has a distinct value ID.
    // SHA3-256 collisions on the same canonical encoding would indicate
    // a broken hash; collisions on different encodings would indicate a
    // canonicalisation bug.
    let cells = corpus();
    let mut by_id: std::collections::HashMap<Hash, Cell> = std::collections::HashMap::new();
    for c in cells {
        if let Some(prev) = by_id.insert(c.value_id(), c.clone()) {
            panic!(
                "value_id collision between {prev:?} and {c:?} (both = {})",
                c.value_id()
            );
        }
    }
}

#[test]
fn corpus_cells_send_across_threads() {
    // Cell: Send + Sync is enforced statically by the lib.rs tripwire,
    // but exercise the round-trip across an actual thread boundary to
    // catch any latent issues.
    let corpus = corpus();
    let other_thread: Vec<_> = std::thread::spawn(move || {
        corpus
            .into_iter()
            .map(|c| (c.value_id(), c.encoding()))
            .collect::<Vec<_>>()
    })
    .join()
    .unwrap();
    for (id, enc) in &other_thread {
        let decoded = Cell::decode(enc).unwrap();
        assert_eq!(&decoded.value_id(), id);
    }
}

#[test]
fn cloned_corpus_preserves_everything() {
    // Clone every corpus cell and re-run the full invariant suite on
    // the clone. Cheap for current variants (all small / bit-copy), but
    // sets the regression once heavy Arc-backed variants land — a
    // clone-bumped Arc Cell must satisfy the same invariants as the
    // original.
    for cell in corpus() {
        let cloned = cell.clone();
        assert_cell_invariants(&cloned);
        // And the clone equals the original.
        assert_eq!(cloned, cell);
        assert_eq!(cloned.value_id(), cell.value_id());
        assert_eq!(cloned.encoding(), cell.encoding());
    }
}
