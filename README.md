# Kadı Atlas — backend

Read-only research API and offline importer for a corpus of ~50–70k historical
Ottoman **kadı (judge) appointment records**. Built with Rust, Axum, Tokio, SQLx
(no ORM), PostgreSQL + PostGIS, Calamine, Serde and Tracing.

- The public HTTP API is **read-only**. There is no write or admin endpoint.
- Data is loaded **only** through the `import_xlsx` CLI.
- The frontend lives in `web/` (plain HTML/CSS/ES modules, no build step). The
  server can serve it on the same origin — see [§5](#5-running-the-api).

See [architecture.md](architecture.md) for the data model, design decisions and
the list of domain questions that need historians' input.

---

## 1. Requirements

- Rust (pinned by [`rust-toolchain.toml`](rust-toolchain.toml): 1.92.0)
- PostgreSQL **16+** with the **PostGIS** extension available
  (`pg_trgm` is also used and ships with PostgreSQL)

## 2. PostgreSQL + PostGIS

### Option A — Docker (simplest)

```bash
docker compose -f compose.dev.yml up -d
# DATABASE_URL=postgres://kadi:kadi@localhost:5432/kadi_atlas
```

### Option B — local PostgreSQL

```bash
# the DB role needs CREATE EXTENSION on first migration; a superuser role is
# simplest for local development
createdb kadi_atlas
psql -d kadi_atlas -c "CREATE EXTENSION IF NOT EXISTS postgis;"
psql -d kadi_atlas -c "CREATE EXTENSION IF NOT EXISTS pg_trgm;"
```

> The migrations also run `CREATE EXTENSION IF NOT EXISTS ...`; creating them by
> hand first just avoids needing DDL-on-extension privileges at migration time.

Set the connection string (either variable name works):

```bash
export DATABASE_URL="postgres://kadi:kadi@localhost:5432/kadi_atlas"
# or copy .env.example -> .env and edit it
cp .env.example .env
```

## 3. Migrations

Migrations live in [`migrations/`](migrations/) and are **embedded in the
binary**. Both the server and the importer apply pending migrations on startup
by default (`KADI_ATLAS__AUTO_MIGRATE=true`).

```bash
# apply migrations without importing anything
cargo run --bin import_xlsx -- --help   # connecting + migrating happens on real runs
# or just start the server once:
cargo run --bin kadi-atlas
```

In production, set `KADI_ATLAS__AUTO_MIGRATE=false` for the API role and run
migrations as a separate deploy step (e.g. a one-off `import_xlsx` invocation or
`sqlx migrate run` with the `migrations/` directory).

## 4. Importing XLSX data

```bash
cargo run --bin import_xlsx -- path/to/data.xlsx
# options:
#   --sheet <name>   pick a worksheet (default: first sheet)
#   --dry-run        parse + validate, then roll back
#   --no-migrate     skip the migration step
```

A tiny sample workbook can be generated for experimentation:

```bash
cargo run --example generate_sample_xlsx -- data/sample.xlsx
cargo run --bin import_xlsx -- data/sample.xlsx
```

Importer behaviour:

- runs in **one transaction** (all-or-nothing) with an advisory lock so two
  imports cannot interleave;
- **idempotent on `doc_id`** — re-importing the same file updates rows in place,
  it never creates duplicates. If a row has no `doc_id`, a stable id is
  synthesized from the sheet name + row number (a warning is printed);
- stores the **verbatim spreadsheet row** in `source_records.raw` (JSONB);
- empty cells become SQL `NULL`; `-`, `yok`, `n/a`, … become `NULL` in the
  normalized columns while the raw row keeps them;
- creates/links `persons` and `places`; recognises coordinate columns
  (`old_latitude/old_longitude`, `new_latitude/new_longitude`) and builds a
  PostGIS point via a trigger; places are still created when coordinates are
  absent, and existing coordinates are never silently overwritten;
- prints a summary with counts and per-row warnings; exits non-zero only on a
  hard failure (the transaction then rolls back).

Recognised column headers are matched case-insensitively with a few common
Turkish aliases (see `column_aliases` in `src/importer.rs`); unrecognised columns
are still preserved in `raw`.

## 5. Running the API

```bash
cargo run --bin kadi-atlas          # dev
./target/release/kadi-atlas         # after: cargo build --release

# API + frontend on one origin: open http://127.0.0.1:8080/
KADI_ATLAS__WEB_DIR=web cargo run --bin kadi-atlas
```

Base path: `/api/v1`. The frontend calls the API on its own origin by
default; set `<meta name="kadi-api-base">` in `web/index.html` to point it
elsewhere. A page opened from `file://` or from a separate local static server
falls back to `http://127.0.0.1:8080/api/v1` (that needs
`KADI_ATLAS__CORS_ALLOW_ORIGIN`).

| Method & path | Purpose |
| --- | --- |
| `GET /api/v1/health` | liveness + DB reachability |
| `GET /api/v1/meta` | corpus counts, `year_numeric` span, calendars, distinct `degrees` / `position_types` |
| `GET /api/v1/persons` | list/search persons (`query`, `resolution_status`, `has_wikidata`, `limit`, `offset`) |
| `GET /api/v1/persons/{id}` | person profile + `journey` in itinerary order (year first, then the old place → new place chain; `sequence`, `follows_previous`, predecessor/successor, coordinates) |
| `GET /api/v1/places` | list/search places (`query`, `has_coordinates`, `has_wikidata`, `limit`, `offset`) |
| `GET /api/v1/places/{id}` | place profile: historical names, stats, related persons |
| `GET /api/v1/appointments` | event list; filters: `person`, `place`, `origin_place`, `destination_place`, `year_from`, `year_to`, `degree`, `position_type`, `region`, `has_coordinates`, `source_record_id`, `limit`, `offset` |
| `GET /api/v1/place-activity` | geocoded places with `arrivals` / `departures` under the `/flows` filters, for the map's place view; `limit` up to 5000 |
| `GET /api/v1/flows` | `origin → destination → count` aggregation for a year window; `year_from`, `year_to`, `person`, `min_count`, `require_coordinates` (default `true`), `limit`, `offset` |
| `GET /api/v1/sources` | source records; `query` runs PostgreSQL full-text search over `source_text` and returns highlighted snippets + `rank` |
| `GET /api/v1/sources/{id}` | one source record including the raw JSON row |

Every list response is `{ "items": [...], "total": N, "limit": N, "offset": N }`.

Errors are `{"error": {"code": "...", "message": "..."}}` with `code` and
`message` **also mirrored at the top level** for simpler clients. Internal
database errors are logged and returned as a generic `500`. Invalid query
parameters → `400`; unknown entity/route → `404`.

**Query-parameter aliases** (accepted in addition to the canonical names, so the
`web/` client's spellings work): `q` → `query` (persons, places, sources);
`person_id` → `person` and `place_id` → `place` (appointments, flows);
`external_id` → `doc_id` and `source_record_id` (sources). `flows` also accepts
`degree` / `position_type`.

### Configuration (environment variables)

See [`.env.example`](.env.example). Key ones:

| Variable | Default | Meaning |
| --- | --- | --- |
| `DATABASE_URL` | — (required) | PostgreSQL/PostGIS connection string |
| `KADI_ATLAS__BIND_ADDR` | `127.0.0.1:8080` | HTTP listen address |
| `KADI_ATLAS__DATABASE_MAX_CONNECTIONS` | `10` | pool size |
| `KADI_ATLAS__DEFAULT_PAGE_SIZE` / `KADI_ATLAS__MAX_PAGE_SIZE` | `50` / `200` | pagination |
| `KADI_ATLAS__REQUEST_TIMEOUT_SECS` | `30` | per-request timeout |
| `KADI_ATLAS__AUTO_MIGRATE` | `true` | run migrations on startup |
| `KADI_ATLAS__LOG_FORMAT` | `pretty` | `pretty` or `json` |
| `RUST_LOG` | `info,...` | tracing filter |
| `KADI_ATLAS__CORS_ALLOW_ORIGIN` | unset | if set, enables a read-only (GET/HEAD) CORS policy for that one origin; otherwise the API is same-origin only |
| `KADI_ATLAS__WEB_DIR` | unset | directory with `index.html` (e.g. `web`) to serve at `/`; unknown `/api/*` paths stay JSON 404s |

Security headers set on every response: `Content-Security-Policy: default-src
'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'` (pages
from `WEB_DIR` get a policy that allows their own scripts, Google Fonts and
same-origin API calls),
`X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy:
no-referrer`, `Permissions-Policy: geolocation=(), camera=(), microphone=(),
interest-cohort=()`, `Cross-Origin-Resource-Policy: same-site`.

## 6. Tests & checks

Integration tests need a database. Point them at a throwaway one:

```bash
export TEST_DATABASE_URL="postgres://kadi:kadi@localhost:5432/kadi_atlas_test"
createdb kadi_atlas_test   # once

cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

The integration suite (`tests/integration.rs`) covers: health, `400` on invalid
queries, `404` on missing entities/routes, XLSX import idempotency (raw
preservation, placeholder → NULL, coordinates → PostGIS, `calendar = unknown`),
person/place search + profiles, appointment filtering (by person and year), and
flow aggregation with coordinates.

## 7. Production build / deploy

```bash
cargo build --release          # profile: opt-level 3, thin LTO, codegen-units 1
```

- Ship `target/release/kadi-atlas` and `target/release/import_xlsx`.
- `Cargo.lock` is committed; the toolchain is pinned — builds are reproducible.
- Run behind a TLS-terminating reverse proxy on the same origin as the frontend
  (no CORS needed).
- Give the API database role **read-only** rights; run migrations and imports
  with a separate role and set `KADI_ATLAS__AUTO_MIGRATE=false` for the API.
- A hardened systemd unit is provided at
  [`deploy/kadi-atlas.service`](deploy/kadi-atlas.service) (SIGTERM →
  graceful drain, `EnvironmentFile` for `DATABASE_URL`).
- Logs go to stdout; set `KADI_ATLAS__LOG_FORMAT=json` for structured logging.
