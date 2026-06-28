#!/usr/bin/env bash
# Run Boa with criticality CSV redirected through a FIFO into Parquet parts.
# Usage:
#   BOA_CRITICALITY_PARQUET_DIR=analysis/criticality/run/parquet \
#     tools/criticality_parquet_wrapper.sh target/release/boa [engine args...]

set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: BOA_CRITICALITY_PARQUET_DIR=DIR $0 ENGINE [engine args...]" >&2
  exit 2
fi
if [[ -z "${BOA_CRITICALITY_PARQUET_DIR:-}" ]]; then
  echo "BOA_CRITICALITY_PARQUET_DIR is required" >&2
  exit 2
fi

engine=$1
shift
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
pipe_dir=${BOA_CRITICALITY_PIPE_DIR:-$(mktemp -d "${TMPDIR:-/tmp}/boa-criticality-pipe.XXXXXX")}
mkdir -p "$pipe_dir" "$BOA_CRITICALITY_PARQUET_DIR"

fifo="$pipe_dir/criticality-$$.fifo"
rm -f "$fifo"
mkfifo "$fifo"

python3 "$script_dir/criticality_to_parquet.py" \
  "$fifo" \
  "$BOA_CRITICALITY_PARQUET_DIR" \
  --stream \
  --part-prefix "$(date +%s)-$$-" \
  --batch-rows "${BOA_CRITICALITY_PARQUET_BATCH_ROWS:-100000}" &
converter=$!

cleanup() {
  rm -f "$fifo"
  rmdir "$pipe_dir" 2>/dev/null || true
}
trap cleanup EXIT

export BOA_CRITICALITY_LOG_FILE="$fifo"
export BOA_CRITICALITY_LOG_DIR="${BOA_CRITICALITY_LOG_DIR:-$pipe_dir}"

"$engine" "$@"
status=$?

# If the engine never opened the criticality log, unblock the FIFO reader so it
# can emit an empty result and exit instead of hanging this wrapper.
: > "$fifo" 2>/dev/null || true
wait "$converter" || status=$?
exit "$status"
