# QPS Benchmarks

```shell
./cacheus > cargo bench --bench qps
./cacheus > grpcurl -plaintext -import-path ./proto -proto hello.proto -d '{"name": "Tonic"}' '127.0.0.1:3001' helloworld.Greeter/SayHello
./cacheus > k6 run benches/qps/k6.js
./cacheus > ghz --insecure --async --proto ./proto/hello.proto --call helloworld.Greeter/SayHello -c 100 -z 10s --rps 50000 -d '{"name":"{{.RequestNumber}}"}' 127.0.0.1:3001
./cacheus > ghz --insecure --async --proto ./proto/hello.proto --call helloworld.Greeter/SayHello -c 100 -z 10s --rps 50000 -d '{"name":"bench"}' 127.0.0.1:3001
```