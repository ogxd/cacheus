use std::io::{Read, Write};
use std::sync::Arc;
use std::{collections::HashMap};
use std::hash::Hash;
use gxhash::{GxBuildHasher, GxHasher};
use serde::{Serialize, Deserialize};
use serde_inline_default::serde_inline_default;
use crate::buffered_body::BufferedBody;
// use crate::{lru, ShardedCache};
use foyer::{*};
use hyper::{HeaderMap, Request, Response, StatusCode};

/// Cache configuration types
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum CacheConfig {
    InMemory {
        in_memory: MemoryCache,
    },
    Hybrid {
        hybrid: HybridCacheConfig,
    },
}

/// In-memory cache configuration
#[serde_inline_default]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemoryCache {
    pub name: String,

    #[serde_inline_default(8)]
    pub shards: usize,

    #[serde_inline_default(128 * 1024 * 1024)]
    pub memory_size_bytes: usize,

    #[serde_inline_default(3600)]
    pub ttl_seconds: u64,

    #[serde_inline_default(true)]
    pub hash_path: bool,

    #[serde_inline_default(true)]
    pub hash_query: bool,

    #[serde_inline_default(true)]
    pub hash_body: bool,
}

#[serde_inline_default]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HybridCacheConfig {
    pub name: String,

    pub path: String,

    #[serde_inline_default(128 * 1024 * 1024)]
    pub memory_size_bytes: usize,

    #[serde_inline_default(128 * 1024 * 1024)]
    pub disk_size_bytes: usize,

    #[serde_inline_default(8)]
    pub shards: usize,

    #[serde_inline_default(3600)]
    pub ttl_seconds: u64,

    #[serde_inline_default(true)]
    pub hash_path: bool,

    #[serde_inline_default(true)]
    pub hash_query: bool,

    #[serde_inline_default(true)]
    pub hash_body: bool,
}

#[derive(Debug, Clone)]
pub struct CachedResponse {
    pub insertion_epoch: u64,
    pub response: Response<BufferedBody>,
}

impl foyer::Code for CachedResponse {
    fn encode(&self, writer: &mut impl Write) -> std::result::Result<(), foyer::CodeError> {
        // Serialize status code (2 bytes)
        let status = self.response.status().as_u16();
        writer.write_all(&status.to_le_bytes()).unwrap();

        // Serialize headers
        let headers = self.response.headers();
        let header_count = headers.len() as u32;
        writer.write_all(&header_count.to_le_bytes()).unwrap();
        for (name, value) in headers.iter() {
            let name_bytes = name.as_str().as_bytes();
            let value_bytes = value.as_bytes();

            writer.write_all(&(name_bytes.len() as u32).to_le_bytes()).unwrap();
            writer.write_all(name_bytes).unwrap();
            writer.write_all(&(value_bytes.len() as u32).to_le_bytes()).unwrap();
            writer.write_all(value_bytes).unwrap();
        }

        // Serialize body
        let body_bytes = self.response.body().to_bytes();
        writer.write_all(&(body_bytes.len() as u64).to_le_bytes()).unwrap();
        writer.write_all(&body_bytes).unwrap();

        // Serialize insertion epoch (8 bytes)
        writer.write_all(&self.insertion_epoch.to_le_bytes()).unwrap();

        Ok(())
    }

    fn decode(reader: &mut impl Read) -> std::result::Result<CachedResponse, foyer::CodeError> {
        // Deserialize status code (2 bytes)
        let mut buf2 = [0u8; 2];
        reader.read_exact(&mut buf2).unwrap();
        let status = StatusCode::from_u16(u16::from_le_bytes(buf2)).map_err(|_| foyer::CodeError::Other("Could not decode status code".into())).unwrap();

        // Deserialize headers
        let mut buf4 = [0u8; 4];
        reader.read_exact(&mut buf4).unwrap();
        let header_count = u32::from_le_bytes(buf4);

        let mut headers = HeaderMap::new();
        for _ in 0..header_count {
            reader.read_exact(&mut buf4).unwrap();
            let name_len = u32::from_le_bytes(buf4) as usize;
            let mut name_buf = vec![0u8; name_len];
            reader.read_exact(&mut name_buf).unwrap();
            let name = String::from_utf8(name_buf).map_err(|_| foyer::CodeError::Other("Could not decode header name".into())).unwrap();

            reader.read_exact(&mut buf4).unwrap();
            let value_len = u32::from_le_bytes(buf4) as usize;
            let mut value_buf = vec![0u8; value_len];
            reader.read_exact(&mut value_buf).unwrap();
            headers.insert(
                hyper::header::HeaderName::try_from(name).map_err(|_| foyer::CodeError::Other("Could not decode header name".into())).unwrap(),
                hyper::header::HeaderValue::from_bytes(&value_buf).map_err(|_| foyer::CodeError::Other("Could not decode header value".into())).unwrap(),
            );
        }

        // Deserialize body
        let mut buf8 = [0u8; 8];
        reader.read_exact(&mut buf8)?;
        let body_len = u64::from_le_bytes(buf8);

        let mut body_buf = vec![0u8; body_len as usize];
        reader.read_exact(&mut body_buf)?;

        let buffered = BufferedBody::from_bytes(&body_buf);

        // Deserialize insertion epoch (8 bytes)
        let mut insertion_epoch_buf = [0u8; size_of::<u64>()];
        reader.read_exact(&mut insertion_epoch_buf).unwrap();
        let insertion_epoch = u64::from_le_bytes(insertion_epoch_buf);

        // Reconstruct response
        let mut response = Response::builder()
            .status(status)
            .body(buffered)
            .map_err(|_| foyer::CodeError::Other("Could not decode body".into())).unwrap();

        *response.headers_mut() = headers;

        Ok(Self { insertion_epoch: insertion_epoch, response })
    }

    fn estimated_size(&self) -> usize {
        let body_len = self.response.body().to_bytes().len();
        let headers_len: usize = self.response.headers().iter().map(|(k, v) | 4 /* name size */ + k.as_str().as_bytes().len() /* name bytes */ + 4 /* value size */ + v.as_bytes().len() /* value bytes */).sum();
        2 /* status code */
        + 4 /* headers count */
        + headers_len /* headers */
        + 8 /* body size */
        + body_len /* body */
        + 8 /* insertion epoch */
    }
}

impl CacheConfig {
    /// Create a cache key from a request
    pub fn create_key(&self, request: &Request<BufferedBody>) -> u128 {
        match self {
            CacheConfig::InMemory { in_memory } => {
                let mut hasher = GxHasher::with_seed(123);
                // Different path/query means different key
                if in_memory.hash_path {
                    request.uri().path().hash(&mut hasher);
                }
                if in_memory.hash_query {
                    if let Some(query) = request.uri().query() {
                        query.hash(&mut hasher);
                    }
                }
                if in_memory.hash_body {
                    request.body().hash(&mut hasher);
                }
                hasher.finish_u128()
            },
            CacheConfig::Hybrid { hybrid: hybrid_config } => {
                let mut hasher = GxHasher::with_seed(123);
                // Different path/query means different key
                if hybrid_config.hash_path {
                    request.uri().path().hash(&mut hasher);
                }
                if hybrid_config.hash_query {
                    if let Some(query) = request.uri().query() {
                        query.hash(&mut hasher);
                    }
                }
                if hybrid_config.hash_body {
                    request.body().hash(&mut hasher);
                }
                hasher.finish_u128()
            }
        }
    }

    /// Add cache to the provided cache map
    pub async fn add_cache(&self, caches: &mut HashMap<String, (CacheConfig, HybridCache<u128, CachedResponse, GxBuildHasher>)>) {

        match self {
            CacheConfig::InMemory { in_memory } => {
               let cache = HybridCacheBuilder::new()
                    .with_name(in_memory.name.clone())
                    .with_flush_on_close(true)
                    .with_policy(foyer::HybridCachePolicy::WriteOnInsertion)
                    .memory(in_memory.memory_size_bytes)
                    .with_hash_builder(gxhash::GxBuildHasher::with_seed(123))
                    .storage(Engine::Large)
                    .with_admission_picker(Arc::new(foyer::RejectAllPicker::default()))
                    .build()
                    .await
                    .unwrap();

                caches.insert(in_memory.name.clone(), (self.clone(), cache));
            },
            CacheConfig::Hybrid { hybrid: hybrid_config } => {
                let hybrid = HybridCacheBuilder::new()
                    .with_name(hybrid_config.name.clone())
                    .with_flush_on_close(true)
                    .with_policy(foyer::HybridCachePolicy::WriteOnInsertion)
                    .memory(hybrid_config.memory_size_bytes)
                    .with_hash_builder(gxhash::GxBuildHasher::with_seed(123))
                    .storage(Engine::Large)
                    .with_admission_picker(Arc::new(foyer::AdmitAllPicker::default()))
                    .with_device_options(
                        DirectFsDeviceOptions::new(hybrid_config.path.clone()).with_capacity(hybrid_config.disk_size_bytes),
                    )
                    .with_flush(true)
                    .with_recover_mode(foyer::RecoverMode::Quiet)
                    .build()
                    .await
                    .unwrap();

                caches.insert(hybrid_config.name.clone(), (self.clone(), hybrid));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use hyper::{Response, header::{HeaderMap, HeaderName, HeaderValue}};
    use foyer::Code;

    #[test]
    fn test_encode_decode_roundtrip() {
        // Prepare test response
        let mut headers = HeaderMap::new();
        headers.insert(HeaderName::from_static("content-type"), HeaderValue::from_static("application/json"));
        headers.insert(HeaderName::from_static("x-test"), HeaderValue::from_static("123"));

        let body_bytes = b"{\"key\":\"value\"}";
        let body = BufferedBody::from_body(body_bytes);

        let response = Response::builder()
            .status(200)
            .body(body)
            .unwrap();

        let cached = CachedResponse {
            insertion_epoch: 0,
            response,
        };

        // Encode
        let mut encoded = Vec::new();
        cached.encode(&mut encoded).unwrap();

        // Decode
        let mut cursor = Cursor::new(encoded);
        let decoded = CachedResponse::decode(&mut cursor).unwrap();

        // Assertions
        assert_eq!(decoded.insertion_epoch, cached.insertion_epoch);
        assert_eq!(decoded.response.status(), cached.response.status());
        assert_eq!(decoded.response.headers().len(), cached.response.headers().len());

        for (k, v) in cached.response.headers().iter() {
            assert_eq!(decoded.response.headers().get(k).unwrap(), v);
        }

        assert_eq!(
            decoded.response.body().to_bytes(),
            cached.response.body().to_bytes()
        );
    }
}