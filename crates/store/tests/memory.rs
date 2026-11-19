//! Runs the store's behavioural suite against the in-memory store.

mod suite;

use venti_store::MemoryStore;

async fn fresh(_test: &str) -> Option<MemoryStore> {
    Some(MemoryStore::new())
}

suite::store_suite!(fresh);
