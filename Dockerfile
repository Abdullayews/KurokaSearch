# KurokaSearch — container image for any Docker-capable host (Render, Fly,
# Railway, a plain VPS). Two stages:
#   1. builder: rust:slim, dependency layer cached separately, cargo capped
#      to a single job so the release build fits small instances (Render's
#      free tier has 512 MB — parallel rustc peaks past that)
#   2. runtime: debian-slim + CA certificates, ~60 MB, non-root user
FROM rust:1-slim AS build
WORKDIR /app

# dependency layer — rebuilt only when Cargo.toml/lock change
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo 'fn main() {}' > src/main.rs \
 && cargo build --release -j 1 \
 && rm -rf src target/release/deps/kuroka_search* target/release/kuroka-search

# real build
COPY src ./src
RUN touch src/main.rs && cargo build --release -j 1

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --no-create-home kuroka

WORKDIR /app
COPY --from=build /app/target/release/kuroka-search ./kuroka-search
COPY --from=build /app/static ./static

# PaaS contract: listen on 0.0.0.0:$PORT (Render sets PORT=10000).
# KUROKA_PROXY / KUROKA_AUTO_PROXY can be added from the host dashboard.
ENV KUROKA_STATIC=/app/static \
    KUROKA_CACHE=/app/cache/img
USER kuroka
EXPOSE 10000
CMD ["sh", "-c", "mkdir -p \"$KUROKA_CACHE\" && exec ./kuroka-search"]
