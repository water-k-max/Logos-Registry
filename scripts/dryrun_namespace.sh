#!/usr/bin/env bash
# Read-only: print lezreg's derived accounts for a self subject and a third-party subject.
set -uo pipefail
cd /home/user/provenance
AUTH=8tWS2X8e59Q4FYUUExxxFzpNjbQirzkFjDjYSukVzsqe
ZERO=$(printf '0%.0s' $(seq 64))
for s in 7b4040d30bc2354f5cfddedaea1a8a088ae6876f375220884485ea7d2e7b0bd6 eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee; do
  cargo run --offline -q -p lezreg -- register \
    --image-id "$s" --authority "$AUTH" \
    --name Demo --version 0.1.0 --author-name Demo --description demo \
    --tags a,b --idl-cid "$ZERO" --dry-run 2>&1 | head -16
  echo '=================='
done
