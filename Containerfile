FROM rust:latest

# Install SQLite dev libraries
RUN apt-get update && apt-get install -y libsqlite3-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the full project
COPY . .

# Build the project and test dependencies
RUN cargo build 2>&1 && cargo test --no-run 2>&1

# Run tests
CMD ["cargo", "test", "--", "--test-threads=1"]
