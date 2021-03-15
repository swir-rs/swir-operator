FROM rust:1.50 as operator-builder
WORKDIR /usr/src/swir
RUN rustup component add rustfmt 
COPY Cargo.toml ./
COPY Cargo.lock ./
COPY src ./src
RUN cargo build --release --all-features


### Split into two files; one to build and one to actually run it
###https://docs.docker.com/develop/develop-images/multistage-build/

FROM debian:buster-slim
RUN apt-get update && apt-get upgrade -y && apt-get install -y  ca-certificates libssl-dev libssl1.1
COPY --from=builder /usr/src/swir/target/release/swir-operator /swir-operator

ENV RUST_BACKTRACE=full
ENV RUST_LOG=info

ENTRYPOINT ["./swir-operator"]