#!/usr/bin/env bash
set -euo pipefail

wasm_path="${1:?usage: $0 PATH_TO_WASM}"
max_bytes="${WASM_MAX_BYTES:-262144}"

[[ -f "$wasm_path" ]] || { echo "Wasm artifact not found: $wasm_path" >&2; exit 1; }
actual_bytes="$(wc -c < "$wasm_path" | tr -d ' ')"
printf 'Wasm size: %s bytes (%.1f KiB), limit: %s bytes\n' \
  "$actual_bytes" "$((actual_bytes / 1024))" "$max_bytes"

if (( actual_bytes > max_bytes )); then
  echo 'Wasm size budget exceeded.' >&2
  exit 1
fi
