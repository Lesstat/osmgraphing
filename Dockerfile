FROM rust:1.48 as planner
WORKDIR app
RUN cargo install cargo-chef
COPY Cargo.toml Cargo.lock /app/
RUN cargo chef prepare  --recipe-path recipe.json

FROM rust:1.48 as cacher
WORKDIR app
RUN cargo install cargo-chef
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

FROM rust:1.48 as builder
WORKDIR app
# Copy over the cached dependencies
COPY --from=cacher /app/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo
COPY . .
ARG GRAPH_DIM=3
RUN cargo build --release --bin osmgraphing

FROM debian:buster-slim as runtime
WORKDIR /
COPY --from=builder /app/target/release/osmgraphing .
ENTRYPOINT ["/osmgraphing"]
