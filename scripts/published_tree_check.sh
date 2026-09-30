#!/usr/bin/env bash
# Does the repository as *published* hash to the source_cid the manifest and the chain name?
#
# `lezbuild verify` answers that about a working copy, which is the thing a reviewer already doubts:
# my disk can be pristine and still not be what GitHub serves. This script is the outside leg -- it
# downloads the archive over plain HTTPS (no credentials, so it fails if the repository is not public),
# unpacks it into a temporary directory, and runs the same verification against THOSE bytes.
#
# Two things are deliberately not asserted here:
#   - that the build reproduces. This script never touches Docker; it answers "same source?" only.
#   - that the numbers in the output stay true. It writes a timestamped log into artifacts/ (which
#     lezbuild prunes from the tree hash), so the point-in-time evidence lives outside the tree it
#     describes instead of turning a documentation edit into a source_cid change.
#
# Run: bash scripts/published_tree_check.sh [archive-url]
# Requires: curl, tar, bc, and the lezbuild binary at tools/lezbuild/target/debug/lezbuild.
set -uo pipefail

SRC=/home/user/provenance
URL=${1:-https://api.github.com/repos/water-k-max/Logos-Registry/tarball/main}
LEZ="$SRC/tools/lezbuild/target/debug/lezbuild"
OUT="$SRC/artifacts/published-tree-check.txt"
WORK=$(mktemp -d /tmp/published-tree-check.XXXXXX)
trap 'rm -rf "$WORK"' EXIT

say() { printf '%s\n' "$*" | tee -a "$OUT"; }

: > "$OUT"
say "# generated $(date -u +%FT%TZ) by scripts/published_tree_check.sh"
say "# archive url: $URL"
say "# local head: $(git -C "$SRC" rev-parse HEAD) (this run checks the published branch, not this commit)"
say

[ -x "$LEZ" ] || { say "ENV FAIL: no lezbuild at $LEZ (build it first)"; exit 3; }

ARCHIVE="$WORK/archive.tar.gz"
HTTP=$(curl -sS -o "$ARCHIVE" -w '%{http_code}' -L "$URL") || { say "CURL FAILED for $URL"; exit 3; }
say "http_status     = $HTTP"
[ "$HTTP" = "200" ] || { say "NOT FETCHABLE ANONYMOUSLY: a private or missing repository returns $HTTP,"; say "and a source_cid pinned to it is not a claim a third party can check."; exit 4; }
say "archive_bytes   = $(stat -c %s "$ARCHIVE")"
say "archive_sha256  = $(sha256sum "$ARCHIVE" | cut -d' ' -f1)"

tar xzf "$ARCHIVE" -C "$WORK" || { say "TAR FAILED"; exit 3; }
SUB=$(ls -1 "$WORK" | grep -v '^archive.tar.gz$' | head -1)
[ -n "$SUB" ] || { say "no top-level directory in the archive"; exit 3; }
say "served_tree     = $SUB"
# GitHub names the archive after the commit it exported, which is the only place this run learns the
# published tip from something other than my own git.
say "served_commit   = ${SUB##*-}"
cd "$WORK/$SUB" || { say "cannot enter $WORK/$SUB"; exit 3; }

say "served_files    = $(find . -type f | wc -l)"
say "served_bytes    = $(find . -type f -printf '%s\n' | paste -sd+ | bc)"
say "gitattributes   = $(tail -1 .gitattributes)   <- must be '-text', or end-of-line conversion"
say "                                        can rewrite the bytes source_cid hashes"

M="$WORK/$SUB/artifacts/reproducible-build.json"
[ -f "$M" ] || { say "no manifest in the published tree"; exit 3; }
say "manifest_source_cid = $(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["source"]["tree_sha256"])' "$M")"
say "manifest_commit     = $(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["source"]["commit"])' "$M")"
say "manifest_file_sha256= $(sha256sum "$M" | cut -d' ' -f1)"
say
say "--- lezbuild verify against the bytes GitHub served ---"
"$LEZ" verify --manifest "$M" 2>&1 | tee -a "$OUT"
RC=${PIPESTATUS[0]}
say "verify_exit     = $RC   (0 = published bytes == the source_cid the manifest claims)"
exit "$RC"
