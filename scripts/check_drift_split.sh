#!/usr/bin/env bash
# After a host-only edit (tools/lezreg, sdk/), prove two things at once:
#   1. the guest graph is untouched -> `verify` still says GUEST GRAPH MATCHES, exit 0
#   2. the whole-tree hash DID move -> `verify --strict` reports it as informational
#      drift and exits 2, while plain `verify` stays green.
# That split is what keeps a verifier usable: a README or a CLI edit must not
# invalidate an ImageID claim, but it must still be visible.
set -uo pipefail
cd /home/user/provenance/tools/lezbuild
cargo build --offline --quiet 2>&1 | tail -3

echo "### verify (blocking checks only)"
./target/debug/lezbuild verify
echo "EXIT=$?"

echo
echo "### verify --strict (tree drift is an error too)"
./target/debug/lezbuild verify --strict 2>&1 | tail -25
echo "EXIT=${PIPESTATUS[0]}"
