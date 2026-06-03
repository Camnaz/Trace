//! Policy Store implementation.
//!
//! Uses DashMap for lock-free concurrent hashmap access and ArcSwap
//! for atomic policy updates without blocking readers.

use std::sync::Arc;
use dashmap::DashMap;
use arc_swap::ArcSwap;
use tracing::{debug, info};

use crate::types::{CustomerId, CustomerPolicy};

/// A thread-safe, lock-free store for customer policies.
pub struct PolicyStore {
    inner: DashMap<CustomerId, ArcSwap<CustomerPolicy>>,
}

impl PolicyStore {
    /// Create a new, empty PolicyStore.
    pub fn new() -> Self {
        Self {
            inner: DashMap::new(),
        }
    }
    
    /// Get a policy for a customer (lock-free read).
    pub async fn get_policy(&self, customer_id: CustomerId) -> Option<Arc<CustomerPolicy>> {
        self.inner
            .get(&customer_id)
            .map(|entry| entry.value().load_full())
    }
    
    /// Set or update a policy for a customer (atomic write).
    pub fn set_policy(&self, customer_id: CustomerId, policy: CustomerPolicy) {
        let arc_policy = Arc::new(policy);
        
        match self.inner.get(&customer_id) {
            Some(entry) => {
                // Update existing entry atomically
                entry.value().store(arc_policy);
                debug!("Updated policy for customer: {}", customer_id.0);
            }
            None => {
                // Insert new entry
                let arc_swap = ArcSwap::new(arc_policy);
                self.inner.insert(customer_id, arc_swap);
                info!("Inserted new policy for customer: {}", customer_id.0);
            }
        }
    }
    
    /// Remove a policy for a customer.
    pub fn remove_policy(&self, customer_id: CustomerId) -> Option<Arc<CustomerPolicy>> {
        self.inner
            .remove(&customer_id)
            .map(|(_, arc_swap)| arc_swap.load_full())
    }
    
    /// Check if a customer has a policy.
    pub fn contains(&self, customer_id: CustomerId) -> bool {
        self.inner.contains_key(&customer_id)
    }
    
    /// Get the number of policies in the store.
    pub fn len(&self) -> usize {
        self.inner.len()
    }
    
    /// Check if the store is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    
    /// Get all customer IDs in the store.
    pub fn customer_ids(&self) -> Vec<CustomerId> {
        self.inner
            .iter()
            .map(|entry| *entry.key())
            .collect()
    }
}

impl Default for PolicyStore {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for PolicyStore {
    fn clone(&self) -> Self {
        // Create a new store and copy all policies
        let new_store = Self::new();
        for entry in self.inner.iter() {
            let policy = entry.value().load_full();
            new_store.inner.insert(*entry.key(), ArcSwap::new(policy));
        }
        new_store
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_policy() -> CustomerPolicy {
        CustomerPolicy {
            customer_id: CustomerId::new(),
            version: "1.0.0".to_string(),
            constraints: vec![],
            default_verdict: crate::types::TraceVerdict::Pass,
            updated_at: chrono::Utc::now(),
        }
    }
    
    #[tokio::test]
    async fn test_insert_and_get() {
        let store = PolicyStore::new();
        let customer_id = CustomerId::new();
        let policy = create_test_policy();
        
        store.set_policy(customer_id, policy.clone());
        
        let retrieved = store.get_policy(customer_id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().version, "1.0.0");
    }
    
    #[tokio::test]
    async fn test_update_policy() {
        let store = PolicyStore::new();
        let customer_id = CustomerId::new();
        
        let policy1 = CustomerPolicy {
            version: "1.0.0".to_string(),
            ..create_test_policy()
        };
        
        let policy2 = CustomerPolicy {
            version: "2.0.0".to_string(),
            ..create_test_policy()
        };
        
        store.set_policy(customer_id, policy1);
        store.set_policy(customer_id, policy2);
        
        let retrieved = store.get_policy(customer_id).await;
        assert_eq!(retrieved.unwrap().version, "2.0.0");
    }
    
    #[tokio::test]
    async fn test_remove_policy() {
        let store = PolicyStore::new();
        let customer_id = CustomerId::new();
        let policy = create_test_policy();
        
        store.set_policy(customer_id, policy);
        assert!(store.contains(customer_id));
        
        store.remove_policy(customer_id);
        assert!(!store.contains(customer_id));
        assert!(store.get_policy(customer_id).await.is_none());
    }
    
    #[test]
    fn test_store_len_and_is_empty() {
        let store = PolicyStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
        
        let customer_id = CustomerId::new();
        let policy = create_test_policy();
        store.set_policy(customer_id, policy);
        
        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
    }
    
    #[tokio::test]
    async fn test_get_nonexistent_policy() {
        let store = PolicyStore::new();
        let customer_id = CustomerId::new();
        
        let result = store.get_policy(customer_id).await;
        assert!(result.is_none());
    }
    
    #[test]
    fn test_store_clone() {
        let store1 = PolicyStore::new();
        let customer_id = CustomerId::new();
        let policy = create_test_policy();
        
        store1.set_policy(customer_id, policy);
        
        let store2 = store1.clone();
        
        // Both stores should have the same policy
        assert_eq!(store1.len(), 1);
        assert_eq!(store2.len(), 1);
        assert!(store1.contains(customer_id));
        assert!(store2.contains(customer_id));
    }
    
    #[test]
    fn test_customer_ids() {
        let store = PolicyStore::new();
        let id1 = CustomerId::new();
        let id2 = CustomerId::new();
        
        store.set_policy(id1, create_test_policy());
        store.set_policy(id2, create_test_policy());
        
        let ids = store.customer_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));
    }
    
    #[tokio::test]
    async fn test_concurrent_reads() {
        let store = PolicyStore::new();
        let customer_id = CustomerId::new();
        let policy = create_test_policy();
        
        store.set_policy(customer_id, policy);
        
        // Spawn multiple concurrent reads
        let mut handles = vec![];
        for _ in 0..10 {
            let store_ref = store.clone();
            let cid = customer_id;
            handles.push(tokio::spawn(async move {
                store_ref.get_policy(cid).await
            }));
        }
        
        // All reads should succeed
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_some());
        }
    }
    
    #[test]
    fn test_default_store() {
        let store: PolicyStore = Default::default();
        assert!(store.is_empty());
    }
}
