FROM rust:latest AS chef
RUN cargo install cargo-chef
RUN apt-get update && apt-get install -y libsqlite3-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --recipe-path recipe.json
COPY . .
RUN cargo build 2>&1 && cargo test --no-run 2>&1

CMD ["cargo", "test", "--", "--test-threads=1"]
