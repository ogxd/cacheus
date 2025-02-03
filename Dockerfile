FROM rust:1.77-alpine3.19 as builder

RUN apk add --no-cache musl-dev pkgconfig openssl-dev
ENV PKG_CONFIG_PATH=/usr/lib/pkgconfig

WORKDIR /usr/src/cacheus
COPY . .

RUN RUSTFLAGS="-C target-feature=+aes" cargo install --path .

FROM alpine:3.19

COPY --from=builder /usr/local/cargo/bin/cacheus /usr/local/bin/cacheus

CMD ["cacheus"]