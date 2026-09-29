#!/usr/bin/env bash
# Launch the post-recipe-edit neutrality check detached, so a WSL shutdown cannot kill it.
# Kept as a file because building CACHE_ID inline with $(date +%s) inside `wsl.exe bash -lc "..."`
# gets mangled by the Windows shell layer (measured: exit 127 on command substitution with parens).
set -uo pipefail
SRC=/home/user/provenance
cd "$SRC" || exit 3
LOG="$SRC/artifacts/outside-envcheck.log"
CACHE_ID="prov-envcheck-$(date +%s)"
echo "launcher: cache id $CACHE_ID, started $(date -u +%FT%TZ)" > "$LOG"
# TREE_CHECK= disables the stale-source leg on purpose: the tree hash is re-baselined at the freeze
# (task #23), so today it would exit 4 before reaching the build. Everything else runs for real.
TREE_CHECK= BUILD=1 CACHE_ID="$CACHE_ID" bash scripts/verify_from_the_outside.sh >> "$LOG" 2>&1
echo "launcher: verifier exit=$? finished $(date -u +%FT%TZ)" >> "$LOG"
