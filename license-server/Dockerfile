# Build stage
FROM rust:slim AS builder

WORKDIR /usr/src/app
COPY . .

# Cài đặt sqlite dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev libsqlite3-dev

# Cài đặt sqlx-cli để prepare (offline)
RUN cargo install sqlx-cli --no-default-features --features rustls,sqlite

# Tạo fake DB để pass sqlx prepare (bước này đã có offline json nếu chạy prepare)
# Nhưng vì ta dùng DATABASE_URL nên tạo tạm
RUN rm -f license.db && touch license.db
ENV DATABASE_URL=sqlite://license.db
RUN sqlx migrate run

RUN cargo build --release --bin license-server

# Production stage
FROM debian:bookworm-slim

WORKDIR /app

RUN apt-get update && apt-get install -y ca-certificates libsqlite3-0 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/app/target/release/license-server .
COPY --from=builder /usr/src/app/migrations ./migrations

# Sẽ tạo file DB thực tế ở runtime qua volume mount nếu chưa có, 
# nhưng DB migrations sẽ được code tự chạy ở app startup.

EXPOSE 3000

ENV DATABASE_URL=sqlite:///app/data/license.db
CMD mkdir -p /app/data && touch /app/data/license.db && ./license-server
