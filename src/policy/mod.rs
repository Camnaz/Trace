//! Policy Store - Lock-free storage for customer constraints.
//!
//! This module provides thread-safe, atomic storage and retrieval
//! of customer policies using lock-free data structures.

pub mod store;

pub use store::PolicyStore;
