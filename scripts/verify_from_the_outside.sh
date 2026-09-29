#!/usr/bin/env bash
# One command, from the outside: (re)build the guest from pinned inputs, then check the result
# against BOTH the build manifest and what the chain actually records.
#
#   scripts/verify_from_the_outside.sh                # compare only: manifest <-> tree <-> chain
#   BUILD=1 scripts/verify_from_the_outside.sh        # + the sealed cold build first (fresh cache id)
#   BUILD=1 BUNDLE=~/prov-seeds/prov-seeds.tar.gz \
#       scripts/verify_from_the_outside.sh            # + rebuild the builder image from the seed bundle
#   TREE_CHECK= scripts/verify_from_the_outside.sh    # skip the "is this the claimed tree" leg
#
# Every expectation is read from artifacts/reproducible-build.json (and, for the bundle, from
# artifacts/seed-bundle.sha256). The script hardcodes no hashes on purpose: a literal that can
# drift from the artifact it describes is exactly how a stale claim starts passing.
#
# Exit codes are part of the contract, because a CI report reads them rather than the prose:
#   0 every claim matched    1 build != manifest    2 chain != manifest (stale or wrong Tier-3)
#   3 environment/usage failure (missing input, docker/cargo/chain unreachable)
#   4 this checkout != the source_cid the manifest claims (the source record is unverifiable)
set -uo pipefail

SRC="${SRC:-/home/user/provenance}"
# Overridable on purpose, and it has to be `${VAR:-default}`: the plain assignment silently beat
# the environment, so every "tamper with the manifest" test case ran against the real one and
# printed VERIFIED. A verifier that ignores its own override cannot have its failure paths tested.
MANIFEST="${MANIFEST:-$SRC/artifacts/reproducible-build.json}"
OUT="$SRC/artifacts/verify-outside"
CTX="${CTX:-/home/user/prov-build-outside}"
BASE_TAG="${BASE_TAG:-r0.1.88.0-provseed-20260928}"
BUILD="${BUILD:-}"
BUNDLE="${BUNDLE:-}"
CACHE_ID="${CACHE_ID:-prov-outside-$(date +%s)}"

die() { printf 'ENV FAIL: %s\n' "$*" >&2; exit 3; }
test -f "$MANIFEST" || die "manifest $MANIFEST missing (lezbuild manifest)"

# ---- expectations, straight out of the manifest -----------------------------
eval "$(python3 - "$MANIFEST" <<'PY'
import json, sys
m = json.load(open(sys.argv[1]))
b, e, s, g = m["build"], m["expect"], m["source"], m["guest_graph"]
print(f'EXP_ELF={e["elf_sha256"]}')
print(f'EXP_SIZE={e["elf_size_bytes"]}')
print(f'EXP_IMAGE={e["image_id"]}')
print(f'EXP_SOURCE_CID={s["tree_sha256"]}')
print(f'EXP_DEP_HASH={g["dep_set_sha256"]}')
print(f'EXP_BUILDER={b["image_digest"].split("@sha256:")[-1]}')
print(f'EXP_REPO_LEN={len(s["repo_url"])}')
PY
)" || die "manifest is not readable JSON"
for v in EXP_ELF EXP_SIZE EXP_IMAGE EXP_SOURCE_CID EXP_DEP_HASH EXP_BUILDER EXP_REPO_LEN; do
    test -n "${!v:-}" || die "manifest has no $v — refusing to compare against a blank expectation"
done
# The manifest's own hash is what the chain stores as manifest_cid, so it must be computed from the
# same file the other expectations came from — never written down twice.
EXP_MANIFEST_CID=$(sha256sum "$MANIFEST" | cut -d' ' -f1)

printf 'manifest        %s\n' "$MANIFEST"
printf '  image id      %s\n' "$EXP_IMAGE"
printf '  elf           %s (%s B)\n' "$EXP_ELF" "$EXP_SIZE"
printf '  source_cid    %s\n' "$EXP_SOURCE_CID"
printf '  manifest_cid  %s (sha256 of the manifest file itself)\n' "$EXP_MANIFEST_CID"
printf '  builder       %s\n' "$EXP_BUILDER"
printf '  dep_audit     %s\n' "$EXP_DEP_HASH"
printf '  repo_url      %s chars expected\n\n' "$EXP_REPO_LEN"

# ---- is THIS tree the tree the manifest (and therefore the chain) names? -----
# source_cid on chain is the manifest's tree_sha256: a hash over every file in the repo except build
# output and the seed caches. Both legs below can pass while that claim is false — a README edit or a
# new script moves the tree but cannot move the guest — and then this script would print VERIFIED
# about a source record no stranger can reconstruct. So check it, before the expensive build, and let
# lezbuild do the hashing: the tree rule (PRUNE set, path encoding, sort order) keeps one
# implementation instead of a second one in shell that can silently diverge.
# NOTE `${TREE_CHECK-1}` without the colon: `${VAR:-1}` also defaults an *empty* value, so the
# documented way to skip this leg (TREE_CHECK=) silently enabled it — caught by the exit-code
# harness, which is the only reason anyone would ever find that.
if [ -n "${TREE_CHECK-1}" ]; then
    VERIFY_OUT=$(cd "$SRC/tools/lezbuild" && cargo run -q --offline -- verify \
        --root "$SRC" --manifest "$MANIFEST" 2>&1)
    TREE_NOW=$(printf '%s\n' "$VERIFY_OUT" | awk '/^source_tree_actual/{print $2; exit}')
    test -n "$TREE_NOW" || die "lezbuild verify printed no source_tree_actual line — rebuild lezbuild"
    if [ "$TREE_NOW" = "$EXP_SOURCE_CID" ]; then
        printf 'source tree     %s\n' "$TREE_NOW"
        printf '                this checkout IS the tree the manifest and the chain name\n\n'
    else
        printf 'SOURCE STALE: the manifest claims source_cid\n'
        printf '                %s\n' "$EXP_SOURCE_CID"
        printf '                but this checkout hashes to\n'
        printf '                %s\n' "$TREE_NOW"
        printf '                The ImageID claim may still hold (that is the guest-graph half, which\n'
        printf '                lezbuild reports above), but the on-chain source record names bytes\n'
        printf '                that are not in this tree, so nobody can verify them. Re-generate the\n'
        printf '                manifest (lezbuild manifest) and re-attach it, or run with TREE_CHECK=\n'
        printf '                to do the build and chain legs anyway.\n'
        exit 4
    fi
fi

# ---- optional: rebuild the builder image from the seed bundle ---------------
if [ -n "$BUNDLE" ]; then
    BUNDLE=$(readlink -f "$BUNDLE") || die "bundle path unresolvable"
    SHA_FILE="$SRC/artifacts/seed-bundle.sha256"
    test -f "$BUNDLE" || die "bundle $BUNDLE missing"
    test -f "$SHA_FILE" || die "expected-bundle hash $SHA_FILE missing"
    ( cd "$(dirname "$BUNDLE")" && sha256sum -c "$SHA_FILE" ) || die "bundle sha256 mismatch"
    STAGE="${STAGE:-/home/user/prov-seed-stage}"
    rm -rf "$STAGE" && mkdir -p "$STAGE"
    tar -xf "$BUNDLE" -C "$STAGE" || die "bundle did not extract"
    # The bundle's own layout is part of its contract: two top-level directories, gitdb/ and
    # regcache/, because that is what build_seeded_builder.sh's completeness guards expect to find.
    GITDB="$STAGE/gitdb"
    REGCACHE="$STAGE/regcache"
    test -d "$GITDB/logos-execution-zone-6bae42d7c9cadfe7" || die "bundle has no gitdb/ LEZ db"
    test -d "$REGCACHE/cache" && test -d "$REGCACHE/index" || die "bundle has no regcache/{cache,index}"
    printf 'bundle extracted %s -> gitdb %s files, regcache %s files\n' "$BUNDLE" \
        "$(find "$GITDB" -type f | wc -l)" "$(find "$REGCACHE" -type f | wc -l)"
    IMG_TAG="${IMG_TAG:-$BASE_TAG}"
    GITDB="$GITDB" REGCACHE="$REGCACHE" IMG_TAG="$IMG_TAG" \
        bash "$SRC/scripts/build_seeded_builder.sh" > "$SRC/artifacts/seeded-image-rebuild.log" 2>&1 \
        || die "seeded image rebuild failed (see artifacts/seeded-image-rebuild.log)"
    BASE_TAG="$IMG_TAG"
    printf 'builder image rebuilt from the bundle: %s\n\n' "$BASE_TAG"
fi

# ---- optional: the sealed cold build ---------------------------------------
BUILT_IMAGE="$EXP_IMAGE"
if [ -n "$BUILD" ]; then
    mkdir -p "$OUT" || die "cannot create $OUT"
    # Same reason as in image_only_reproduction.sh: docker leaves the exported ELF's mtime as
    # recorded inside the image, so a previous run's bytes would otherwise be indistinguishable
    # from this run's. Delete first, then require the build to have produced them.
    rm -f "$OUT/provenance.elf" "$OUT/provenance.bin"
    SEEDED=1 BASE_TAG="$BASE_TAG" CACHE_ID="$CACHE_ID" NETWORK=none \
        bash "$SRC/scripts/build_guest.sh" "$CTX" "$OUT" > "$OUT/run.log" 2>&1 \
        || { tail -20 "$OUT/run.log"; die "build_guest.sh failed"; }
    test -f "$OUT/build.log" || die "build_guest.sh produced no build.log"
    test -f "$OUT/provenance.elf" || die "build exited 0 but exported no ELF"
    BUILT_IMAGE=$(grep -oE 'IMAGEID [0-9a-f]{64}' "$OUT/build.log" | tail -1 | cut -d' ' -f2)
    GOT_ELF=$(sha256sum "$OUT/provenance.elf" | cut -d' ' -f1)
    GOT_SIZE=$(stat -c %s "$OUT/provenance.elf")
    printf 'cold build      cache id %s, network none, base %s\n' "$CACHE_ID" "$BASE_TAG"
    printf '  elf           %s (%s B)\n' "$GOT_ELF" "$GOT_SIZE"
    printf '  image id      %s\n' "${BUILT_IMAGE:-<none produced>}"
    build_ok=1
    [ "$GOT_ELF" = "$EXP_ELF" ] || { printf '  MISMATCH elf hash vs manifest\n'; build_ok=0; }
    [ "$GOT_SIZE" = "$EXP_SIZE" ] || { printf '  MISMATCH elf size vs manifest\n'; build_ok=0; }
    [ "$BUILT_IMAGE" = "$EXP_IMAGE" ] || { printf '  MISMATCH ImageID vs manifest\n'; build_ok=0; }
    [ "$build_ok" = 1 ] || exit 1
    printf '  BUILD MATCHES the manifest\n\n'
else
    printf 'cold build      SKIPPED (set BUILD=1).\n'
    printf '                The chain check below is the manifest against the registry record only;\n'
    printf '                it does not prove this machine can produce the bytes.\n\n'
fi

# ---- the chain read: does the registry record agree with the manifest? ------
cd "$SRC/sdk" || die "cannot enter $SRC/sdk"
CHAIN=$(cargo run --offline -q --example explorer-resolve -- "$BUILT_IMAGE" 2>&1)
CHAIN_RC=$?
if [ "$CHAIN_RC" -eq 3 ]; then die "chain says $BUILT_IMAGE is UNREGISTERED (exit 3)"; fi
if [ "$CHAIN_RC" -ne 0 ]; then printf '%s\n' "$CHAIN" | tail -5; die "explorer-resolve exited $CHAIN_RC"; fi

field() { printf '%s\n' "$CHAIN" | awk -v k="$1" '$1==k{print $2; exit}'; }
CHAIN_STATUS=$(field status)
chain_ok=1
check() { # label chain_value expected_value
    if [ "$2" = "$3" ]; then printf '  ok            %-14s %s\n' "$1" "$2"
    else printf '  MISMATCH      %-14s chain=%s manifest=%s\n' "$1" "$2" "$3"; chain_ok=0; fi
}

printf 'chain           %s (tip block %s)\n' "$(field rpc)" \
    "$(printf '%s\n' "$CHAIN" | awk '/^tip block/{print $NF; exit}')"
printf '  entry account %s   status %s   revision %s\n' \
    "$(printf '%s\n' "$CHAIN" | awk '/^entry account/{print $3; exit}')" "$CHAIN_STATUS" "$(field revision)"
[ "$CHAIN_STATUS" = "REGISTERED" ] || { printf '  FAIL          not registered\n'; chain_ok=0; }
check source_cid "$(field source_cid)" "$EXP_SOURCE_CID"
check manifest_cid "$(field manifest_cid)" "$EXP_MANIFEST_CID"
check builder "$(field builder_digest)" "$EXP_BUILDER"
check dep_audit "$(field dep_audit_hash)" "$EXP_DEP_HASH"
# repo_url and commit are deliberately unset on chain until the source is published somewhere a
# third party can resolve: a commit hash with no referent would be a false claim, so a zero here is
# the expected value, not an oversight. Length only, because the printed value is free text — which
# means a wrong URL of the right length would pass this line; the URL itself is checked by eye.
# NOTE the `$1==` form: the record fields are indented two spaces, so /^repo_url/ never matches.
check repo_url_len "$(printf '%s\n' "$CHAIN" | awk '$1=="repo_url"{print length($2); exit}')" "$EXP_REPO_LEN"

if [ "$chain_ok" = 1 ]; then
    printf '\nVERIFIED: the on-chain Tier-3 record matches the manifest'
    [ -n "$BUILD" ] && printf ', which matches the bytes this machine just built'
    [ -n "${TREE_CHECK-1}" ] && printf ', which matches this checkout file for file'
    printf '.\n'
    exit 0
fi
printf '\nSTALE OR WRONG: chain and manifest disagree.\n'
printf 'Either the manifest moved and was never re-attached (lezreg attach-manifest --send), or the\n'
printf 'build inputs drifted. Do not publish a "reproducible" claim on top of this.\n'
exit 2
