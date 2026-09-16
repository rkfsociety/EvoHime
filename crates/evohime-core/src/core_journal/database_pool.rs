use std::{
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
};

use evohime_local_storage::{LocalDatabase, StorageError};

use super::WORKSPACE_DATABASE_POOL_SIZE;

struct PoolState {
    idle: Vec<LocalDatabase>,
    total: usize,
    generation: u64,
}

/// A small blocking pool for long-running workspace RAG operations.
///
/// Migration is completed before this pool is constructed. Connections are
/// opened lazily up to a fixed bound and returned to the pool after each
/// operation, so a search never invokes the schema installers.
pub(crate) struct PreparedDatabasePool {
    path: PathBuf,
    max_size: usize,
    state: Mutex<PoolState>,
    available: Condvar,
}

pub(crate) struct PreparedDatabaseLease {
    pool: Arc<PreparedDatabasePool>,
    database: Option<LocalDatabase>,
    generation: u64,
}

impl PreparedDatabasePool {
    pub(crate) fn new(path: impl AsRef<Path>) -> Result<Arc<Self>, StorageError> {
        let path = path.as_ref().to_path_buf();
        let database = LocalDatabase::open_prepared(&path)?;
        Ok(Arc::new(Self {
            path,
            max_size: WORKSPACE_DATABASE_POOL_SIZE,
            state: Mutex::new(PoolState {
                idle: vec![database],
                total: 1,
                generation: 0,
            }),
            available: Condvar::new(),
        }))
    }

    pub(crate) fn checkout(self: &Arc<Self>) -> Result<PreparedDatabaseLease, StorageError> {
        loop {
            let mut state = self.state.lock().map_err(|_| {
                StorageError::InvalidInput("workspace database pool is poisoned".into())
            })?;
            if let Some(database) = state.idle.pop() {
                return Ok(PreparedDatabaseLease {
                    pool: Arc::clone(self),
                    database: Some(database),
                    generation: state.generation,
                });
            }
            if state.total < self.max_size {
                state.total += 1;
                let generation = state.generation;
                drop(state);
                match LocalDatabase::open_prepared(&self.path) {
                    Ok(database) => {
                        let mut state = self.state.lock().map_err(|_| {
                            StorageError::InvalidInput("workspace database pool is poisoned".into())
                        })?;
                        if state.generation != generation {
                            state.total = state.total.saturating_sub(1);
                            self.available.notify_all();
                            continue;
                        }
                        return Ok(PreparedDatabaseLease {
                            pool: Arc::clone(self),
                            database: Some(database),
                            generation,
                        });
                    }
                    Err(error) => {
                        let mut state = self.state.lock().map_err(|_| {
                            StorageError::InvalidInput("workspace database pool is poisoned".into())
                        })?;
                        state.total = state.total.saturating_sub(1);
                        self.available.notify_one();
                        return Err(error);
                    }
                }
            }
            state = self.available.wait(state).map_err(|_| {
                StorageError::InvalidInput("workspace database pool is poisoned".into())
            })?;
            drop(state);
        }
    }

    /// Drops idle connections and fences in-flight connections from being
    /// returned after a database restore replaces the SQLite file.
    pub(crate) fn invalidate(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.generation = state.generation.wrapping_add(1);
        state.total = state.total.saturating_sub(state.idle.len());
        state.idle.clear();
        self.available.notify_all();
    }

    #[cfg(test)]
    fn idle_len(&self) -> usize {
        self.state.lock().expect("pool state").idle.len()
    }
}

impl PreparedDatabaseLease {
    pub(crate) fn database_mut(&mut self) -> &mut LocalDatabase {
        self.database.as_mut().expect("database lease is present")
    }
}

impl Drop for PreparedDatabaseLease {
    fn drop(&mut self) {
        let Some(database) = self.database.take() else {
            return;
        };
        let Ok(mut state) = self.pool.state.lock() else {
            return;
        };
        if self.generation == state.generation {
            state.idle.push(database);
        } else {
            state.total = state.total.saturating_sub(1);
        }
        self.pool.available.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn database_path() -> PathBuf {
        std::env::temp_dir().join(format!("evohime-prepared-pool-{}.db", std::process::id()))
    }

    #[test]
    fn reuses_connections_and_stays_bounded() {
        let path = database_path();
        let _ = fs::remove_file(&path);
        let _database = LocalDatabase::open(&path).expect("database opens");
        let pool = PreparedDatabasePool::new(&path).expect("pool opens");
        assert_eq!(pool.idle_len(), 1);

        let first = pool.checkout().expect("first checkout");
        assert_eq!(pool.idle_len(), 0);
        drop(first);
        assert_eq!(pool.idle_len(), 1);

        let mut leases = Vec::new();
        for _ in 0..WORKSPACE_DATABASE_POOL_SIZE {
            leases.push(pool.checkout().expect("bounded checkout"));
        }
        assert_eq!(pool.idle_len(), 0);
        drop(leases);
        assert_eq!(pool.idle_len(), WORKSPACE_DATABASE_POOL_SIZE);

        let stale = pool.checkout().expect("stale checkout");
        pool.invalidate();
        drop(stale);
        assert_eq!(pool.idle_len(), 0);
        let fresh = pool.checkout().expect("fresh checkout");
        drop(fresh);
        assert_eq!(pool.idle_len(), 1);

        drop(pool);
        drop(_database);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("db-wal"));
        let _ = fs::remove_file(path.with_extension("db-shm"));
    }
}
