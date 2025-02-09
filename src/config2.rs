use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Configuration {
    pub listening_port: u16,
    pub prometheus_port: u16,
    pub https: bool,
    pub http2: bool,
    pub minimum_log_level: String,
    pub default_target_host: String,
    pub caches: Vec<Cache>,
    pub rules: Vec<Rule>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Cache {
    pub in_memory: InMemoryCache,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InMemoryCache {
    pub name: String,
    pub probatory_size: usize,
    pub resident_size: usize,
    pub cache_ttl_seconds: u64,
    pub hash_body: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    pub conditions: Vec<Condition>,
    pub actions: Vec<Action>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Condition {
    Any { any: Vec<Condition> },
    All { all: Vec<Condition> },
    HeaderExists { header_exists: String },
    PathContains { path_contains: String },
    StatusCode { status_code: StatusCodeCondition },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StatusCodeCondition {
    pub value: u16,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Action {
    Block { block: BlockAction },
    Add { add: AddAction },
    BodyReplace { body_replace: BodyReplaceAction },
    Cache { cache: CacheAction },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BlockAction {
    pub status_code: u16,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddAction {
    pub headers: Vec<Header>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BodyReplaceAction {
    pub find: String,
    pub replacement: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheAction {
    pub cache_name: String,
}

#[cfg(test)]
mod tests
{
    use super::*;

    // #[test]
    // fn test_config_deserialization()
    // {
    //     let conf = "in_memory_shards: 42\n\
    //                 cache_resident_size: 123\n\
    //                 cache_probatory_size: 456\n\
    //                 listening_port: 789\n\
    //                 http2: false";

    //     let configuration: Configuration = serde_yaml::from_str::<Configuration>(conf).unwrap();

    //     assert_eq!(configuration.in_memory_shards, 42);
    //     assert_eq!(configuration.cache_resident_size, 123);
    //     assert_eq!(configuration.cache_probatory_size, 456);
    //     assert_eq!(configuration.listening_port, 789);
    // }
}
