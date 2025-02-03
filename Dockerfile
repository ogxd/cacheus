FROM rust:1.84-slim-bullseye as builder

RUN apt-get update
RUN apt-get install pkg-config libssl-dev -y

WORKDIR /usr/src/cacheus
COPY . .

RUN RUSTFLAGS="-C target-feature=+aes" cargo install --path .

FROM debian:bullseye-slim

COPY --from=builder /usr/local/cargo/bin/cacheus /usr/local/bin/cacheus

CMD ["cacheus"]