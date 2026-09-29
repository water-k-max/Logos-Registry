#!/usr/bin/env bash
# Poll the registry entry until the attach-manifest tx is visible, then print the record.
# Read-only: getAccount per poll, no submission. Block cadence on the testnet is ~50-90 s.
set -uo pipefail
IMAGE=7b4040d30bc2354f5cfddedaea1a8a088ae6876f375220884485ea7d2e7b0bd6
cd ~/provenance/sdk
for i in $(seq 1 "${POLLS:-12}"); do
    out=$(cargo run --offline -q --example explorer-resolve -- "$IMAGE" 2>&1)
    rev=$(printf '%s\n' "$out" | grep '^  revision' | tr -d ' ')
    if printf '%s\n' "$out" | grep -q 'source_cid      4d37847b'; then
        printf 'APPLIED after %s polls (%s)\n' "$i" "$rev"
        printf '%s\n' "$out" | tail -20
        exit 0
    fi
    printf 'poll %s: not yet (%s)\n' "$i" "$rev"
    sleep 15
done
printf 'NOT APPLIED after %s polls; last read:\n' "${POLLS:-12}"
printf '%s\n' "$out" | tail -20
exit 1
