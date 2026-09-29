#!/usr/bin/env bash
# Measured: how fast does the testnet produce blocks? Decides the SDK's default
# write-confirmation timeout and poll interval. Read-only.
set -uo pipefail
RPC="${RPC:-https://testnet.lez.logos.co/}"
for i in 1 2 3 4 5 6 7; do
    T=$(date +%s)
    B=$(curl -s -m 30 -X POST "$RPC" -H 'content-type: application/json' \
        -d '{"jsonrpc":"2.0","id":1,"method":"getLastBlockId","params":{}}' \
        | python3 -c 'import json,sys; print(json.load(sys.stdin)["result"])')
    echo "$T block=$B"
    sleep 12
done
