#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

HOST="${HOST:-127.0.0.1}"
PORT="${PORT:-3777}"
SKIP_BUILD=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --host)
      HOST="$2"
      shift 2
      ;;
    --host=*)
      HOST="${1#--host=}"
      shift
      ;;
    --port)
      PORT="$2"
      shift 2
      ;;
    --port=*)
      PORT="${1#--port=}"
      shift
      ;;
    --skip-build)
      SKIP_BUILD=1
      shift
      ;;
    *)
      echo "Unknown option: $1" >&2
      echo "Usage: ./run-web.sh [--host 127.0.0.1] [--port 3777] [--skip-build]" >&2
      exit 2
      ;;
  esac
done

if [[ ! -d node_modules ]]; then
  echo "Installing backend dependencies..."
  npm install
fi

if [[ ! -d web/node_modules ]]; then
  echo "Installing frontend dependencies..."
  npm --prefix web install
fi

if [[ "$SKIP_BUILD" != "1" ]]; then
  echo "Building web frontend..."
  npm run web:build
  echo "Building backend..."
  npm run build
fi

echo
echo "Starting Boa Match Manager at http://${HOST}:${PORT}"
echo "Press Ctrl-C to stop."
echo

exec node dist/cli.js web --host "$HOST" --port "$PORT"
