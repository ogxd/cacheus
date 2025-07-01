# QPS Benchmarks

```shell
./cacheus > cargo bench --bench qps
./cacheus > curl http://127.0.0.1:3001
./cacheus > k6 run --out dashboard benches/qps_http/k6.js
```