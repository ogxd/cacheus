# Cacheus

Cacheus is a blazingly fast and ultra-efficient multi-protocol read-through caching proxy. Placed in front of backend services, Cacheus will cache responses and dramatically reduce the load, without involving any changes on the client side, unlike other caching solutions.
- **Multi-protocol**: Cacheus has no API. It simply supports HTTP/1.1 and HTTP/2, including everything based on (such as unary gRPC). Just point your client to Cacheus, and it will take care of the rest.
- **Content-agnostic**: Cacheus does not care about the content of the responses, it will cache anything. Whether it's JSON, XML, HTML, or even binary data like protobuf encoded messages, Cacheus will cache it.
- **Read-through caching**: In case of a cache miss, Cacheus will transparently forward the request to the backend service, cache the response, and return it to the client. This removes the need for handling cache misses in the client, and makes Cacheus a drop-in solution.
- **Blazingly fast**: Cacheus uses a mixture of the best algorithms and data structures (gxhash for high throughput, in-memory sharded cache for improved concurrency, arena-based linked list for memory-efficient LRU, ...) to provide the best performance possible. This makes Cacheus so fast that it will consume orders of magnitude less resources than the services it's caching, making a real difference.

## Usage

Call google.com through Cacheus:
```bash
# todo
```

### WIP - How to tell Cacheus which service to reach

How to handle faulty services? Cacheus is not meant to be a load balance or a service discovery tool. It must take forward the requests as-is. 

#### Pass host as path
It looks weird but it shouldn't imply any change on the client side. Host url is usually just a configuration parameter.  
Might not work however with most gRPC clients, as we only pass the host and no path.
```bash
curl http://localhost:8080/google.com?hello`
```

#### Pass host as header
It involves a (small) change on the client side, but it's the most flexible solution.
```bash
curl --header "X-TargetHost: google.com?hello" http://localhost:8080`
```

#### Pass host as config
Issue is that it cannot change dynamically

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

