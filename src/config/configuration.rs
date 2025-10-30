use serde::{Serialize, Deserialize};
use serde_inline_default::serde_inline_default;
use super::{CacheConfig, MiddlewareEnum};

/// Main server configuration structure
#[serde_inline_default]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Configuration {
    #[serde_inline_default(3001)]
    pub listening_port: u16,
    #[serde_inline_default(8000)]
    pub prometheus_port: u16,
    #[serde_inline_default(8001)]
    pub healthcheck_port: u16,
    #[serde_inline_default("info".to_string())]
    pub minimum_log_level: String,
    #[serde(default)]
    pub middlewares: Vec<MiddlewareEnum>,
    #[serde(default)]
    pub caches: Vec<CacheConfig>,
}
