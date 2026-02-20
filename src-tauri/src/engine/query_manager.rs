// SPDX-License-Identifier: Apache-2.0

//! Query Manager
//!
//! Tracks active queries per session and provides query IDs for cancellation.

use std::collections::{HashMap, HashSet};

use tokio::sync::RwLock;

use crate::engine::types::{QueryId, SessionId};

pub struct QueryManager {
    active: RwLock<HashMap<QueryId, SessionId>>,
    by_session: RwLock<HashMap<SessionId, HashSet<QueryId>>>,
    last_by_session: RwLock<HashMap<SessionId, QueryId>>,
}

impl QueryManager {
    pub fn new() -> Self {
        Self {
            active: RwLock::new(HashMap::new()),
            by_session: RwLock::new(HashMap::new()),
            last_by_session: RwLock::new(HashMap::new()),
        }
    }

    pub async fn register(&self, session_id: SessionId) -> QueryId {
        let query_id = QueryId::new();
        let _ = self.register_with_id(session_id, query_id).await;
        query_id
    }

    pub async fn register_with_id(
        &self,
        session_id: SessionId,
        query_id: QueryId,
    ) -> Result<QueryId, String> {
        {
            let mut active = self.active.write().await;
            if active.contains_key(&query_id) {
                return Err("Query ID already registered".to_string());
            }
            active.insert(query_id, session_id);
        }

        {
            let mut by_session = self.by_session.write().await;
            by_session
                .entry(session_id)
                .or_insert_with(HashSet::new)
                .insert(query_id);
        }

        {
            let mut last = self.last_by_session.write().await;
            last.insert(session_id, query_id);
        }

        Ok(query_id)
    }

    pub async fn finish(&self, query_id: QueryId) {
        let session_id = {
            let mut active = self.active.write().await;
            active.remove(&query_id)
        };

        if let Some(session_id) = session_id {
            let mut by_session = self.by_session.write().await;
            if let Some(set) = by_session.get_mut(&session_id) {
                set.remove(&query_id);
                if set.is_empty() {
                    by_session.remove(&session_id);
                }
            }

            let mut last = self.last_by_session.write().await;
            if last.get(&session_id) == Some(&query_id) {
                last.remove(&session_id);
            }
        }
    }

    pub async fn contains(&self, query_id: QueryId) -> bool {
        let active = self.active.read().await;
        active.contains_key(&query_id)
    }

    pub async fn session_for(&self, query_id: QueryId) -> Option<SessionId> {
        let active = self.active.read().await;
        active.get(&query_id).copied()
    }

    pub async fn last_for_session(&self, session_id: SessionId) -> Option<QueryId> {
        let last = self.last_by_session.read().await;
        last.get(&session_id).copied()
    }
}

impl Default for QueryManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn registers_and_finishes_queries() {
        let manager = QueryManager::new();
        let session = SessionId::new();
        let query_id = manager.register(session).await;

        assert!(manager.contains(query_id).await);
        assert_eq!(manager.session_for(query_id).await, Some(session));
        assert_eq!(manager.last_for_session(session).await, Some(query_id));

        manager.finish(query_id).await;
        assert!(!manager.contains(query_id).await);
    }

    #[tokio::test]
    async fn rejects_duplicate_query_id() {
        let manager = QueryManager::new();
        let session = SessionId::new();
        let query_id = QueryId::new();

        manager
            .register_with_id(session, query_id)
            .await
            .expect("first registration should succeed");

        let err = manager
            .register_with_id(session, query_id)
            .await
            .expect_err("duplicate should fail");

        assert!(err.contains("already"));
    }
}
