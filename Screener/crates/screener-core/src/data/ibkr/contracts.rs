use std::collections::HashMap;
use std::future::Future;

use tokio::sync::RwLock;

use crate::error::ScreeningError;

#[derive(Debug, Clone, PartialEq)]
pub struct QualifiedContract {
    pub symbol: String,
    pub contract_id: i32,
}

#[derive(Default)]
pub struct ContractCache {
    cache: RwLock<HashMap<String, QualifiedContract>>,
}

impl ContractCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the cached qualified contract for `symbol` if present;
    /// otherwise calls `qualify` to resolve it and caches the result on
    /// success. A failed qualification is not cached, so it will be retried
    /// on the next call for the same symbol.
    ///
    /// `qualify` takes an owned `String` rather than `&str` — tying `Fut`'s
    /// lifetime to a borrowed parameter here runs into a well-known Rust
    /// limitation with generic `FnOnce(&str) -> Fut` signatures (the
    /// compiler can't express that `Fut` may borrow from the argument
    /// without higher-ranked trait bounds that generic async closures don't
    /// support cleanly); taking ownership sidesteps it entirely.
    pub async fn get_or_qualify<F, Fut>(&self, symbol: &str, qualify: F) -> Result<QualifiedContract, ScreeningError>
    where
        F: FnOnce(String) -> Fut,
        Fut: Future<Output = Result<QualifiedContract, ScreeningError>>,
    {
        if let Some(cached) = self.cache.read().await.get(symbol) {
            return Ok(cached.clone());
        }

        let qualified = qualify(symbol.to_string()).await?;
        self.cache.write().await.insert(symbol.to_string(), qualified.clone());
        Ok(qualified)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn qualifies_a_symbol_on_first_lookup() {
        let cache = ContractCache::new();
        let result = cache
            .get_or_qualify("AAPL", |symbol| async move {
                Ok(QualifiedContract { symbol: symbol.to_string(), contract_id: 42 })
            })
            .await
            .unwrap();
        assert_eq!(result.contract_id, 42);
    }

    #[tokio::test]
    async fn caches_the_result_and_does_not_requalify() {
        let cache = ContractCache::new();
        let call_count = Arc::new(AtomicUsize::new(0));

        for _ in 0..3 {
            let call_count = Arc::clone(&call_count);
            cache
                .get_or_qualify("AAPL", move |symbol| {
                    call_count.fetch_add(1, Ordering::SeqCst);
                    async move { Ok(QualifiedContract { symbol: symbol.to_string(), contract_id: 42 }) }
                })
                .await
                .unwrap();
        }

        assert_eq!(call_count.load(Ordering::SeqCst), 1, "qualify should only run once, on the first call");
    }

    #[tokio::test]
    async fn different_symbols_are_qualified_independently() {
        let cache = ContractCache::new();
        let call_count = Arc::new(AtomicUsize::new(0));

        for symbol in ["AAPL", "MSFT"] {
            let call_count = Arc::clone(&call_count);
            cache
                .get_or_qualify(symbol, move |s| {
                    call_count.fetch_add(1, Ordering::SeqCst);
                    let s = s.to_string();
                    async move { Ok(QualifiedContract { symbol: s.clone(), contract_id: 1 }) }
                })
                .await
                .unwrap();
        }

        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_failed_qualification_is_not_cached() {
        let cache = ContractCache::new();
        let call_count = Arc::new(AtomicUsize::new(0));

        for _ in 0..2 {
            let call_count = Arc::clone(&call_count);
            let result = cache
                .get_or_qualify("BADSYM", move |symbol| {
                    call_count.fetch_add(1, Ordering::SeqCst);
                    let symbol = symbol.to_string();
                    async move { Err(ScreeningError::ContractQualificationFailed(symbol)) }
                })
                .await;
            assert!(result.is_err());
        }

        assert_eq!(call_count.load(Ordering::SeqCst), 2, "a failed qualification should be retried, not cached");
    }
}
