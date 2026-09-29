#!/usr/bin/env bash
set -uo pipefail
RPC=https://testnet.lez.logos.co/
for A in "$@"; do
  echo "--- $A"
  curl -s --max-time 25 -X POST $RPC -H 'content-type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getAccount\",\"params\":{\"account_id\":\"$A\"}}" \
    > "/home/user/e2acct-$A.json"
  python3 /home/user/e2_show.py "/home/user/e2acct-$A.json"
done
