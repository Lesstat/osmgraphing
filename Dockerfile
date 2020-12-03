FROM rust:1.48 as planner
WORKDIR app
RUN cargo install cargo-chef
COPY Cargo.toml Cargo.lock /app/
RUN cargo chef prepare  --recipe-path recipe.json

FROM rust:1.48 as cacher
WORKDIR app
RUN cargo install cargo-chef
COPY --from=planner /app/recipe.json recipe.json

RUN apt-get update
RUN apt-get install -y libcgal-dev libeigen3-dev
RUN cargo chef cook --features='gpl' --release --recipe-path recipe.json

FROM rust:1.48 as builder
WORKDIR app
# Copy over the cached dependencies
RUN apt-get update
RUN apt-get install -y libcgal-dev libeigen3-dev
COPY --from=cacher /app/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo
COPY . .
ARG GRAPH_DIM=3
RUN cargo build --release --bin osmgraphing --features='gpl'

FROM debian:buster-slim as runtime
WORKDIR /
RUN apt-get update
RUN apt-get install -y libcgal-dev libeigen3-dev

COPY --from=builder /app/target/release/osmgraphing .
CMD ["/osmgraphing"]
