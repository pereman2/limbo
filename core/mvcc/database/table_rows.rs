use super::{MVTableId, RowID, RowKey, RowVersions};
use crate::alloc::{ConcurrentAllocator, HashMap, TryReserveError};
use crate::sync::RwLock;
use rustc_hash::FxHasher;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::ops::Bound;

const SHARD_COUNT: usize = 64;
const SHARD_MASK: usize = SHARD_COUNT - 1;

#[derive(Debug)]
struct IntRowHash<A: ConcurrentAllocator> {
    shards: Box<[RwLock<HashMap<(MVTableId, i64), RowVersions<A>>>]>,
}

impl<A: ConcurrentAllocator> IntRowHash<A> {
    fn new() -> Self {
        Self {
            shards: (0..SHARD_COUNT)
                .map(|_| RwLock::new(HashMap::default()))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        }
    }

    fn get(&self, table_id: MVTableId, rowid: i64) -> Option<RowVersions<A>> {
        self.shards[shard(table_id, rowid)]
            .read()
            .get(&(table_id, rowid))
            .cloned()
    }

    fn with_shard_write<R>(
        &self,
        table_id: MVTableId,
        rowid: i64,
        f: impl FnOnce(&mut HashMap<(MVTableId, i64), RowVersions<A>>) -> R,
    ) -> R {
        let mut shard = self.shards[shard(table_id, rowid)].write();
        f(&mut shard)
    }

    fn clear(&self) {
        for shard in self.shards.iter() {
            shard.write().clear();
        }
    }
}

fn shard(table_id: MVTableId, rowid: i64) -> usize {
    let mut hasher = FxHasher::default();
    table_id.hash(&mut hasher);
    rowid.hash(&mut hasher);
    hasher.finish() as usize & SHARD_MASK
}

/// INTEGER PRIMARY KEY point lookup is a sharded hash. Ordered scans, GC, and
/// checkpoint walk the B-tree map. Neither path inserts `RowKey::Int` into the
/// table-row SkipMap.
#[derive(Debug)]
pub struct TableRows<A: ConcurrentAllocator> {
    hash: IntRowHash<A>,
    ordered: RwLock<BTreeMap<RowID, RowVersions<A>>>,
}

pub struct TableRowRef<A: ConcurrentAllocator> {
    key: RowID,
    versions: RowVersions<A>,
}

impl<A: ConcurrentAllocator> TableRowRef<A> {
    pub fn key(&self) -> &RowID {
        &self.key
    }

    pub fn value(&self) -> &RowVersions<A> {
        &self.versions
    }
}

pub struct TableRowRange<A: ConcurrentAllocator> {
    rows: *const TableRows<A>,
    start: Bound<RowID>,
    end: Bound<RowID>,
    reverse: bool,
}

unsafe impl<A: ConcurrentAllocator> Send for TableRowRange<A> {}
unsafe impl<A: ConcurrentAllocator> Sync for TableRowRange<A> {}

impl<A: ConcurrentAllocator> TableRows<A> {
    pub fn new() -> Self {
        Self {
            hash: IntRowHash::new(),
            ordered: RwLock::new(BTreeMap::new()),
        }
    }

    pub fn get(&self, id: &RowID) -> Option<TableRowRef<A>> {
        let versions = match id.row_id {
            RowKey::Int(rowid) => self.hash.get(id.table_id, rowid)?,
            RowKey::Record(_) => self.ordered.read().get(id).cloned()?,
        };
        Some(TableRowRef {
            key: id.clone(),
            versions,
        })
    }

    pub fn insert(&self, id: RowID, versions: RowVersions<A>) -> TableRowRef<A> {
        match id.row_id {
            RowKey::Int(rowid) => {
                self.hash.with_shard_write(id.table_id, rowid, |shard| {
                    shard.insert((id.table_id, rowid), versions.clone());
                    self.ordered.write().insert(id.clone(), versions.clone());
                });
            }
            RowKey::Record(_) => {
                self.ordered.write().insert(id.clone(), versions.clone());
            }
        }
        TableRowRef { key: id, versions }
    }

    pub fn try_get_or_insert_with<F>(
        &self,
        id: RowID,
        value_fn: F,
    ) -> Result<TableRowRef<A>, TryReserveError>
    where
        F: FnOnce() -> RowVersions<A>,
    {
        if let Some(existing) = self.get(&id) {
            return Ok(existing);
        }
        match id.row_id {
            RowKey::Int(rowid) => {
                let mut value_fn = Some(value_fn);
                self.hash.with_shard_write(id.table_id, rowid, |shard| {
                    if let Some(existing) = shard.get(&(id.table_id, rowid)) {
                        return Ok(TableRowRef {
                            key: id.clone(),
                            versions: existing.clone(),
                        });
                    }
                    let versions = value_fn.take().expect("create closure runs once")();
                    shard.insert((id.table_id, rowid), versions.clone());
                    self.ordered
                        .write()
                        .entry(id.clone())
                        .or_insert_with(|| versions.clone());
                    Ok(TableRowRef {
                        key: id.clone(),
                        versions,
                    })
                })
            }
            RowKey::Record(_) => {
                let mut tree = self.ordered.write();
                if let Some(existing) = tree.get(&id) {
                    return Ok(TableRowRef {
                        key: id,
                        versions: existing.clone(),
                    });
                }
                let versions = value_fn();
                tree.insert(id.clone(), versions.clone());
                Ok(TableRowRef { key: id, versions })
            }
        }
    }

    pub fn remove(&self, id: &RowID) {
        match id.row_id {
            RowKey::Int(rowid) => {
                self.hash.with_shard_write(id.table_id, rowid, |shard| {
                    shard.remove(&(id.table_id, rowid));
                    self.ordered.write().remove(id);
                });
            }
            RowKey::Record(_) => {
                self.ordered.write().remove(id);
            }
        }
    }

    pub fn clear(&self) {
        self.ordered.write().clear();
        self.hash.clear();
    }

    pub fn len(&self) -> usize {
        self.ordered.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.ordered.read().is_empty()
    }

    pub fn iter(&self) -> TableRowRange<A> {
        self.range((Bound::<RowID>::Unbounded, Bound::<RowID>::Unbounded))
    }

    pub fn range<R>(&self, bounds: R) -> TableRowRange<A>
    where
        R: std::ops::RangeBounds<RowID>,
    {
        TableRowRange {
            rows: self as *const Self,
            start: bound_cloned(bounds.start_bound()),
            end: bound_cloned(bounds.end_bound()),
            reverse: false,
        }
    }
}

impl<A: ConcurrentAllocator> TableRowRange<A> {
    pub fn rev(mut self) -> Self {
        self.reverse = !self.reverse;
        self
    }
}

impl<A: ConcurrentAllocator> Iterator for TableRowRange<A> {
    type Item = TableRowRef<A>;

    fn next(&mut self) -> Option<Self::Item> {
        // SAFETY: `rows` points at the `TableRows` inside `MvStore`. Cursors
        // and checkpoint hold that store for the iterator's lifetime.
        let rows = unsafe { &*self.rows };
        let tree = rows.ordered.read();
        let picked = if self.reverse {
            tree.range((self.start.clone(), self.end.clone()))
                .rev()
                .next()
                .map(|(key, versions)| (key.clone(), versions.clone()))
        } else {
            tree.range((self.start.clone(), self.end.clone()))
                .next()
                .map(|(key, versions)| (key.clone(), versions.clone()))
        };
        drop(tree);
        let (key, versions) = picked?;
        if self.reverse {
            self.end = Bound::Excluded(key.clone());
        } else {
            self.start = Bound::Excluded(key.clone());
        }
        Some(TableRowRef { key, versions })
    }
}

fn bound_cloned(bound: Bound<&RowID>) -> Bound<RowID> {
    match bound {
        Bound::Included(id) => Bound::Included(id.clone()),
        Bound::Excluded(id) => Bound::Excluded(id.clone()),
        Bound::Unbounded => Bound::Unbounded,
    }
}
