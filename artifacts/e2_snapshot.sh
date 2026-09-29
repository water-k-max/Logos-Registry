#!/usr/bin/env bash
set -uo pipefail
RPC=https://testnet.lez.logos.co/
ENTRY=Hm4Yzd2vgTCvhvPFQUzLT2xoYidRK84QYBzPLQnH8dbJ
for A in $ENTRY 8tWS2X8e59Q4FYUUExxxFzpNjbQirzkFjDjYSukVzsqe A4nuRThP7GiUeLZZn2eRemHjZLaVjMyL6RNtveCBst7j; do
  echo "--- $A"
  curl -s -X POST $RPC -H 'content-type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getAccount\",\"params\":{\"account_id\":\"$A\"}}" \
    > "/home/user/e2acct-$A.json"
  python3 /home/user/e2_show.py "/home/user/e2acct-$A.json"
done
echo "--- tip"
curl -s -X POST $RPC -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getLastBlockId","params":{}}'
