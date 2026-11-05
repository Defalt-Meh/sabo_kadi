# Kadı Atlas — single container: API + frontend on one origin.
# Works on any Docker host (Render, Koyeb, Railway, a VPS...). The database is
# external (e.g. a free Neon PostgreSQL + PostGIS project) via DATABASE_URL.

# ---- 1. build the Rust binaries ------------------------------------------
FROM rust:1.92-bookworm AS build
WORKDIR /src
# rust-toolchain.toml would make rustup fetch extra components; the image
# already ships the pinned 1.92 compiler, which is all a release build needs.
ENV RUSTUP_TOOLCHAIN=1.92.0
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
COPY src ./src
COPY examples ./examples
COPY tests ./tests
RUN cargo build --release --locked --bin kadi-atlas --bin import_xlsx

# ---- 2. synthetic demo workbook (used only if no real data is supplied) ---
FROM python:3.12-slim AS fixture
COPY scripts/make_fixture.py /make_fixture.py
RUN python3 /make_fixture.py demo /demo.xlsx

# ---- 3. runtime ----------------------------------------------------------
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home /app kadi
WORKDIR /app
COPY --from=build /src/target/release/kadi-atlas /src/target/release/import_xlsx /app/bin/
COPY web /app/web
COPY deploy/seed /app/seed
COPY --from=fixture /demo.xlsx /app/seed-demo/demo.xlsx
COPY deploy/start.sh /app/start.sh
RUN chmod +x /app/start.sh && chown -R kadi /app
USER kadi

ENV KADI_ATLAS__WEB_DIR=/app/web \
    KADI_ATLAS__LOG_FORMAT=json \
    KADI_ATLAS__DATABASE_MAX_CONNECTIONS=5 \
    RUST_LOG=info,kadi_atlas=info,tower_http=warn,sqlx=warn \
    PORT=10000
EXPOSE 10000
CMD ["/app/start.sh"]
