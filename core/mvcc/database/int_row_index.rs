use super::{MVTableId, RowVersions};
use crate::alloc::{ConcurrentAllocator, HashMap};
use crate::sync::RwLock;
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};

const SHARD_COUNT: usize = 64;
const SHARD_MASK: usize = SHARD_COUNT - 1;

#[derive(Debug)]
pub(super) struct IntRowIndex<A: ConcurrentAllocator> {
    shards: Box<[RwLock<HashMap<(MVTableId, i64), RowVersions<A>>>]>,
}

impl<A: ConcurrentAllocator> IntRowIndex<A> {
    pub(super) fn new() -> Self {
        Self {
            shards: (0..SHARD_COUNT)
                .map(|_| RwLock::new(HashMap::default()))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        }
    }

    pub(super) fn get(&self, table_id: MVTableId, rowid: i64) -> Option<RowVersions<A>> {
        self.shards[shard(table_id, rowid)]
            .read()
            .get(&(table_id, rowid))
            .cloned()
    }

    pub(super) fn insert(&self, table_id: MVTableId, rowid: i64, versions: RowVersions<A>) {
        self.shards[shard(table_id, rowid)]
            .write()
            .insert((table_id, rowid), versions);
    }

    pub(super) fn remove(&self, table_id: MVTableId, rowid: i64) {
        self.shards[shard(table_id, rowid)]
            .write()
            .remove(&(table_id, rowid));
    }
}

fn shard(table_id: MVTableId, rowid: i64) -> usize {
    let mut hasher = FxHasher::default();
    table_id.hash(&mut hasher);
    rowid.hash(&mut hasher);
    hasher.finish() as usize & SHARD_MASK
}
