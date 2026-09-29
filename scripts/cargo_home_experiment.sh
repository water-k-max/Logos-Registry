#!/usr/bin/env bash
# Does CARGO_HOME alone move the deployed ImageID?
#
# Three arms, identical source + lock + base image + rustflags + CARGO_TARGET_DIR, one variable:
#   1. /root/.cargo   -- CONTROL. Must reproduce a1a0fdc8.../7b4040d3..., or what is being compared
#                        is not the deployed recipe and the rest of the table is meaningless.
#   2. /root/.carg1   -- SAME LENGTH (12 chars) as the control, different bytes. If the ImageID moves
#                        here, the cause is the path *content* that file!() embeds, not its length.
#   3. /opt/alt-cargo -- 14 chars, two longer. If .rodata grows by ~2 bytes x (files recorded), the
#                        length effect is quantified separately from the content effect.
#
# Why it matters: §0.12 measured that `cargo vendor` builds a DIFFERENT program from identical
# source, and blamed the materialization path by inspection. Inspection is not proof. If this test
# passes, "the same source can only be reproduced at one cargo home layout" is a demonstrated
# property of LEZ guests -- which is also the leading candidate explanation for upstream's own
# inability to rebuild token_mint_authority (§0.9), and the reason a registry like LP-0023 has to
# record the builder image, not just the lockfile.
#
# Nothing here is a claim about the pinned recipe: docker/experiment-cargo-home.Dockerfile is a
# separate file and docker/build-guest.Dockerfile is not read or modified by this script.
set -uo pipefail
SRC=/home/user/provenance
CTX=/home/user/prov-build-cargohome
OUTROOT=/home/user/provenance/artifacts/verify-cargohome
BASE_TAG="${BASE_TAG:-r0.1.88.0-provseed-bundle1}"
PACKAGER="$SRC/target/debug/risc0-packager"
IMGID="${IMGID:-/home/user/imgid/target/release/imgid}"

eval "$(python3 - "$SRC/artifacts/reproducible-build.json" <<'PY'
import json, sys
e = json.load(open(sys.argv[1]))["expect"]
print(f'EXP_ELF={e["elf_sha256"]}')
print(f'EXP_SIZE={e["elf_size_bytes"]}')
print(f'EXP_IMAGE={e["image_id"]}')
PY
)" || { echo "ABORT: cannot read expectations from the manifest"; exit 3; }

test -x "$PACKAGER" || { echo "ABORT: packager missing (cargo build -p risc0-packager)"; exit 3; }
test -x "$IMGID"    || { echo "ABORT: imgid missing at $IMGID"; exit 3; }
docker info >/dev/null || { echo "ABORT: docker daemon unreachable"; exit 3; }

echo "==> staging $SRC -> $CTX (no target/, no .git/, no artifacts/)"
rm -rf "$CTX"; mkdir -p "$CTX"
rsync -a --exclude 'target/' --exclude '.git/' --exclude 'artifacts/' "$SRC"/ "$CTX"/
cp "$SRC/docker/experiment-cargo-home.Dockerfile" "$CTX/experiment-cargo-home.Dockerfile"

printf '%-16s %-6s %-14s %-12s %s\n' CARGO_HOME chars ELF12 IMAGEID12 path-strings
rm -f "$OUTROOT/table.txt"; mkdir -p "$OUTROOT"

for CH in /root/.cargo /root/.carg1 /opt/alt-cargo; do
    tag=$(printf '%s' "$CH" | tr -d '/.')
    OUT="$OUTROOT/$tag"
    rm -rf "$OUT"; mkdir -p "$OUT"
    echo "==> arm CARGO_HOME=$CH (network none, base $BASE_TAG)"
    DOCKER_BUILDKIT=1 docker build \
      -f "$CTX/experiment-cargo-home.Dockerfile" \
      --build-arg "ALT_CARGO_HOME=$CH" \
      --build-arg "RISC0_DOCKER_CONTAINER_TAG=$BASE_TAG" \
      --network none \
      --output="$OUT" \
      "$CTX" > "$OUT/build.log" 2>&1
    rc=$?
    if [ "$rc" -ne 0 ] || [ ! -f "$OUT/provenance.elf" ]; then
        printf '%-16s BUILD FAILED exit %s (tail below)\n' "$CH" "$rc"
        tail -12 "$OUT/build.log" | sed 's/^/    | /'
        continue
    fi
    "$PACKAGER" "$OUT/provenance.elf" "$OUT/provenance.bin" > "$OUT/pack.log" 2>&1
    ID=$(cut -d' ' -f1 < <("$IMGID" "$OUT/provenance.bin"))
    ELF=$(sha256sum "$OUT/provenance.elf" | cut -d' ' -f1)
    SIZE=$(stat -c %s "$OUT/provenance.elf")
    NSTR=$(grep -oE 'ARM-PATHSTRINGS [0-9]+' "$OUT/build.log" | tail -1 | cut -d' ' -f2)
    SEC=""
    if command -v readelf >/dev/null; then
        rodata=$(readelf -S -W "$OUT/provenance.elf" 2>/dev/null | awk '/\.rodata/{print $6}')
        text=$(readelf -S -W "$OUT/provenance.elf" 2>/dev/null | awk '/\.text/{print $6}')
        SEC=".text $text .rodata $rodata"
    fi
    verdict="DIFFERENT"
    [ "$ELF" = "$EXP_ELF" ] && [ "$ID" = "$EXP_IMAGE" ] && verdict="= DEPLOYED"
    printf '%-16s %-6s %-14s %-12s %-8s %s %s\n' "$CH" "${#CH}" "${ELF:0:12}" "${ID:0:12}" "$NSTR" "$verdict" "$SEC"
    printf '%s %s %s %s %s\n' "$CH" "$ELF" "$SIZE" "$ID" "${NSTR:-NA}" >> "$OUTROOT/table.txt"
done

echo
echo "deployed (from the manifest, never hardcoded here):"
printf '  ELF %s (%s B)\n  ImageID %s\n' "$EXP_ELF" "$EXP_SIZE" "$EXP_IMAGE"
if grep -q "^/root/.cargo " "$OUTROOT/table.txt" 2>/dev/null && \
   CTRL=$(awk '$1=="/root/.cargo"{print $4}' "$OUTROOT/table.txt") && \
   [ "$CTRL" = "$EXP_IMAGE" ]; then
    echo "CONTROL HOLDS: the unmodified cargo home reproduced the deployed ImageID in this harness."
    others=$(awk '$1!="/root/.cargo"' "$OUTROOT/table.txt" | wc -l)
    moved=$(awk -v id="$EXP_IMAGE" '$1!="/root/.cargo" && $4!=id' "$OUTROOT/table.txt" | wc -l)
    samelen=$(awk -v id="$EXP_IMAGE" '$1=="/root/.carg1" && $4!=id' "$OUTROOT/table.txt" | wc -l)
    echo "RELOCATION TEST: $moved of $others relocated arms changed the ImageID."
    echo "SAME-LENGTH ARM (/root/.carg1, 12 chars like the control): moved=$samelen"
    if [ "$moved" = "$others" ] && [ "$others" -gt 0 ]; then
        echo "CAUSATION ESTABLISHED: CARGO_HOME is a build input, and not because of its length."
    elif [ "$moved" = 0 ]; then
        echo "CAUSATION NOT SUPPORTED by this experiment: the vendor result must have another cause."
    else
        echo "MIXED RESULT: read the table, do not quote a verdict."
    fi
else
    echo "CONTROL FAILED: this harness does not reproduce the deployed ImageID at the stock cargo"
    echo "home, so no comparison below it is valid. Fix the harness before drawing conclusions."
    exit 1
fi
