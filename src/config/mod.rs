//! Configuration module for Cacheus server
//! 
//! This module contains all configuration-related structures and logic,
//! organized into logical submodules.

pub mod server;
pub mod request;
pub mod response;
pub mod cache;
pub mod conditions;

// Re-export main types for convenience
pub use server::Configuration;
pub use request::{OnRequest, BlockRequest, LookupCache, ForwardRequest};
pub use response::{OnResponse, StoreCache, AddResponseHeader};
pub use cache::{CacheConfig, MemoryCache};
pub use conditions::OnRequestCondition;
