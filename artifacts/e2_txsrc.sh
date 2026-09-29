#!/usr/bin/env bash
set -uo pipefail
cd "$HOME/.cargo/git/checkouts/logos-execution-zone-6bae42d7c9cadfe7/47eba25" || exit 1
echo "=== where is indexer_service_protocol?"
find . -maxdepth 4 -type d -name 'protocol' -path '*indexer*'
F=$(grep -rl 'pub struct Transaction' --include=*.rs . | head -3)
echo "=== files declaring Transaction: $F"
for f in $F; do
  echo "--- $f"
  grep -n -A 24 'pub struct Transaction' "$f"
done
