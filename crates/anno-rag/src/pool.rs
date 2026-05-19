//! Bounded pool of reusable heavy resources (e.g. NER engines). An
//! `acquire().await` hands out an RAII guard; dropping it returns the
//! item to the pool. Concurrency is bounded by the pool size.

use std::sync::Mutex;
use tokio::sync::Semaphore;

/// Fixed-size pool of `T`. `acquire` blocks when all are checked out.
pub struct Pool<T> {
    items: Mutex<Vec<T>>,
    sem: Semaphore,
    size: usize,
}

/// RAII handle; returns the item to the pool on drop.
pub struct PoolGuard<'a, T> {
    pool: &'a Pool<T>,
    val: Option<T>,
}

impl<T> Pool<T> {
    /// Build a pool from `items` (must be non-empty in practice).
    #[must_use]
    pub fn new(items: Vec<T>) -> Self {
        let size = items.len();
        Self {
            items: Mutex::new(items),
            sem: Semaphore::new(size),
            size,
        }
    }

    /// Number of resources in the pool (fixed at construction).
    #[must_use]
    pub fn size(&self) -> usize {
        self.size
    }

    /// Acquire one resource, awaiting a free slot. Never panics in
    /// normal use (semaphore is never closed).
    pub async fn acquire(&self) -> PoolGuard<'_, T> {
        let permit = self
            .sem
            .acquire()
            .await
            .expect("pool semaphore is never closed");
        permit.forget(); // slot lifetime is tied to PoolGuard::drop
        let val = self
            .items
            .lock()
            .expect("pool mutex poisoned")
            .pop()
            .expect("a permit implies an available item");
        PoolGuard {
            pool: self,
            val: Some(val),
        }
    }
}

impl<T> std::ops::Deref for PoolGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.val.as_ref().expect("guard holds a value until drop")
    }
}

impl<T> Drop for PoolGuard<'_, T> {
    fn drop(&mut self) {
        if let Some(v) = self.val.take() {
            self.pool.items.lock().expect("pool mutex poisoned").push(v);
            self.pool.sem.add_permits(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn acquire_returns_distinct_items_and_bounds_concurrency() {
        let pool = Pool::new(vec![1u32, 2u32]);
        assert_eq!(pool.size(), 2);
        let a = pool.acquire().await;
        let b = pool.acquire().await;
        assert_ne!(*a, *b, "two acquires give the two distinct items");

        // Third acquire must block while both are checked out.
        let blocked = tokio::time::timeout(Duration::from_millis(50), pool.acquire()).await;
        assert!(blocked.is_err(), "3rd acquire blocks until a release");

        drop(a);
        let c = tokio::time::timeout(Duration::from_millis(200), pool.acquire())
            .await
            .expect("acquire succeeds after a release");
        assert!(*c == 1 || *c == 2);
    }
}
