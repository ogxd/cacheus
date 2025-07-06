# Cacheus

```
⚠️ This is a work in progress. The API and features are subject to change.
```

Cacheus is a blazingly fast and ultra-efficient multi-protocol read-through caching proxy. Placed in front of backend services, Cacheus will cache responses and dramatically reduce the load, without involving any changes on the client side, unlike other caching solutions.
- **Multi-protocol**: Cacheus has no API. It simply supports HTTP/1.1 and HTTP/2, including everything based on (such as unary gRPC). Just point your client to Cacheus, and it will take care of the rest.
- **Content-agnostic**: Cacheus does not care about the content of the responses, it will cache anything. Whether it's JSON, XML, HTML, or even binary data like protobuf encoded messages, Cacheus will cache it.
- **Read-through caching**: In case of a cache miss, Cacheus will transparently forward the request to the backend service, cache the response, and return it to the client. This removes the need for handling cache misses in the client, and makes Cacheus a drop-in solution.
- **Blazingly fast**: Cacheus uses a mixture of the best algorithms and data structures (gxhash for high throughput, in-memory sharded cache for improved concurrency, arena-based linked list for memory-efficient LRU, ...) to provide the best performance possible. This makes Cacheus so fast that it will consume orders of magnitude less resources than the services it's caching, making a real difference.

## Usage

Here is an example configuration file for Cacheus:
```yaml
listening_port: 3001
prometheus_port: 8081
https: false
http2: false
minimum_log_level: info

# Middlewares are applied in the order they are defined.
middlewares:
  - block_request:
      # Every middleware can have conditions.
      # If conditions are met, the middleware will apply its logic, otherwise it will just hand the request to the next middleware.
      when: 
        not:
          path_contains: '*download'
  - use_cache:
      # Will lookup in the cache and return it when cache hit.
      # Will store the response in the cache when cache miss.
      cache_name: files
  - forward_request:
      # Finally forward the request to the target service.
      # Happens unless the request was blocked or cached in a previous middleware.
      target_host: google.com

# We can define multiple caches and of different types.
caches:
  - in_memory:
      name: files
      ttl_seconds: 60
      hash_path: true
      hash_query: true
      hash_body: false
```

Call google.com through Cacheus:
```bash
# todo
```

## Todo

- [x] Setup a way to test Cacheus against various targets
- [x] Support HTTP/1.1
- [x] Support HTTP/2
- [x] Support https
- [x] Properly route
- [x] Add basic logging
- [x] Expose prometheus metrics
- [x] Setup and run benchmarks
- [x] Setup CI
- [x] Implement arena-based linked list
- [x] Implement LRU cache
- [x] Implement probatory LRU cache
- [x] Implemented in-memory sharding
- [x] Use gxhash for sharding and keying
- [x] Implement actual caching in Cacheus
- [ ] Design hot keys cluster sharing
- [x] Find out how Cacheus will know service to reach
- [x] Find out how to configure service
- [ ] Experiment with bytedance/monoio
- [x] Modular configuration based on middlewares
- [ ] Sequence diagram generation from configuration
- [ ] Dynamic configuration reloading
- [ ] Benchmark against nginx, traefik, envoy, etc.

