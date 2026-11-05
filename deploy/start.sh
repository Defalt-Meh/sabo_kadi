#!/bin/sh
# Container entrypoint: start the API, then load data once if the DB is empty.
#
# Data source, first match wins:
#   1. KADI_SEED_URL   - a direct download link to an .xlsx workbook
#   2. /app/seed/*.xlsx - workbooks committed under deploy/seed/
#   3. /app/seed-demo/demo.xlsx - small SYNTHETIC demo set (not real records)
# Set KADI_SEED=off to never import automatically.
set -eu

PORT="${PORT:-10000}"
export KADI_ATLAS__BIND_ADDR="${KADI_ATLAS__BIND_ADDR:-0.0.0.0:$PORT}"
BASE="http://127.0.0.1:${PORT}/api/v1"

/app/bin/kadi-atlas &
SERVER_PID=$!
trap 'kill -TERM "$SERVER_PID" 2>/dev/null' TERM INT

seed() {
  [ "${KADI_SEED:-on}" = "off" ] && return 0

  # wait for the API (migrations run on its startup)
  i=0
  until curl -fsS "$BASE/health" >/dev/null 2>&1; do
    i=$((i + 1)); [ "$i" -gt 120 ] && { echo "seed: API never became healthy" >&2; return 1; }
    sleep 1
  done

  sources=$(curl -fsS "$BASE/meta" | sed -n 's/.*"sources":\([0-9]*\).*/\1/p')
  if [ "${sources:-0}" != "0" ]; then
    echo "seed: database already has $sources source records, skipping import"
    return 0
  fi

  files=""
  if [ -n "${KADI_SEED_URL:-}" ]; then
    mkdir -p /tmp/seed
    echo "seed: downloading workbook from KADI_SEED_URL"
    curl -fsSL "$KADI_SEED_URL" -o /tmp/seed/data.xlsx && files="/tmp/seed/data.xlsx"
  fi
  if [ -z "$files" ]; then
    files=$(ls /app/seed/*.xlsx 2>/dev/null || true)
  fi
  if [ -z "$files" ]; then
    echo "seed: no real workbook supplied, loading the SYNTHETIC demo set"
    files=/app/seed-demo/demo.xlsx
  fi

  for f in $files; do
    echo "seed: importing $f"
    /app/bin/import_xlsx "$f" || echo "seed: import of $f failed" >&2
  done
}

seed &
wait "$SERVER_PID"
