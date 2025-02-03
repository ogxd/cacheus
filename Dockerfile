FROM rust:1.84-slim-bullseye as builder

RUN apt-get update && \
    apt-get install pkg-config libssl-dev -y

WORKDIR /usr/src/cacheus
COPY . .

RUN RUSTFLAGS="-C target-feature=+aes" cargo install --path .

FROM debian:bullseye-slim

# Install the runtime OpenSSL libraries
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates libssl1.1 && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/local/cargo/bin/cacheus /usr/local/bin/cacheus

ENV RUST_BACKTRACE=1

CMD ["cacheus"]