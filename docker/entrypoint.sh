#!/bin/bash
set -e

# Everything the server writes lives under ./data, the directory to mount.
DB_PATH="./data/ferrite.db"
# Demo mode: wipe the database and uploads every N seconds (0 = never).
RESET_INTERVAL="${DB_RESET_INTERVAL:-0}"

SERVER=(./ferrite-server
    --http 0.0.0.0:4000
    --db "$DB_PATH"
    --elf-dir ./data/elfs
    --firmware-dir ./data/firmware
    --static-dir ./dashboard)

if [ "$RESET_INTERVAL" -le 0 ]; then
    exec "${SERVER[@]}"
fi

# The server keeps its SQLite file open, so deleting the file underneath it
# resets nothing. Stop the server, delete its data, start again.
pid=""
timer=""
stop() {
    [ -n "$timer" ] && kill "$timer" 2>/dev/null
    [ -n "$pid" ] && kill -TERM "$pid" 2>/dev/null && wait "$pid"
    exit 0
}
trap stop TERM INT

while true; do
    "${SERVER[@]}" &
    pid=$!
    sleep "$RESET_INTERVAL" &
    timer=$!
    finished=""
    rc=0
    wait -n -p finished "$pid" "$timer" || rc=$?
    if [ "$finished" = "$pid" ]; then
        kill "$timer" 2>/dev/null || true
        echo "[$(date -u)] ferrite-server exited with status $rc"
        exit "$rc"
    fi
    echo "[$(date -u)] Resetting demo database..."
    kill -TERM "$pid"
    wait "$pid" || true
    rm -f "$DB_PATH" "${DB_PATH}-wal" "${DB_PATH}-shm"
    rm -rf ./data/elfs ./data/firmware
done
