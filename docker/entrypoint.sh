#!/bin/bash
set -e

DB_PATH="./data/ferrite.db"
RESET_INTERVAL="${DB_RESET_INTERVAL:-7200}" # 2 hours in seconds

# Background: reset DB on interval
if [ "$RESET_INTERVAL" -gt 0 ]; then
    (
        while true; do
            sleep "$RESET_INTERVAL"
            echo "[$(date -u)] Resetting demo database..."
            rm -f "$DB_PATH" "${DB_PATH}-wal" "${DB_PATH}-shm"
            echo "[$(date -u)] Database reset complete — server will recreate on next request"
        done
    ) &
fi

exec ./ferrite-server \
    --http 0.0.0.0:4000 \
    --db "$DB_PATH" \
    --elf-dir ./elfs \
    --static-dir ./dashboard
