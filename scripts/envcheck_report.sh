#!/usr/bin/env bash
# Did the declared CARGO_HOME actually reach (a) the manifest lezbuild parses and (b) the container
# that reproduced the ImageID? Two separate claims, two separate measurements.
set -uo pipefail
SRC=/home/user/provenance

echo "=== 1. the pinned recipe declares it ==="
grep -n "ENV CARGO_HOME" "$SRC/docker/build-guest.Dockerfile" || echo "  NOT PRESENT"

echo "=== 2. lezbuild parses it into build.env (manifest written to /tmp, pinned manifest untouched) ==="
cd "$SRC/tools/lezbuild" || exit 3
cargo run -q --offline -- manifest --offline --out /tmp/m-envcheck.json > /tmp/m-envcheck.log 2>&1
echo "  manifest_exit=$?"
python3 - <<'PY'
import json
m = json.load(open("/tmp/m-envcheck.json"))
env = m.get("build", {}).get("env", {})
print("  build.env keys:", sorted(env.keys()))
print("  CARGO_HOME =", env.get("CARGO_HOME", "<ABSENT>"))
print("  image_digest =", m["build"].get("image_digest", "")[:20], "...")
print("  placeholders still pending?", [k for k, v in env.items() if "pending" in str(v)])
PY

echo "=== 3. the verifier run that reproduced the ImageID, with the recipe as edited ==="
grep -E "cold build|BUILD|VERIFIED|image id|exit=" "$SRC/artifacts/outside-envcheck.log" | sed 's/^/  /'
OUTDIR=$(find "$SRC/artifacts" -maxdepth 1 -type d -name 'verify-outside*' | head -1)
echo "  build output dir: ${OUTDIR:-none}"
if [ -n "${OUTDIR:-}" ]; then
    printf '  Compiling lines: %s\n' "$(grep -c '^ *Compiling' "$OUTDIR/build.log" 2>/dev/null || echo '?')"
    grep -oE 'IMAGEID [0-9a-f]{64}' "$OUTDIR/build.log" 2>/dev/null | sed 's/^/  /'
    sha256sum "$OUTDIR/provenance.elf" 2>/dev/null | sed 's/^/  elf /'
    stat -c '  elf bytes %s' "$OUTDIR/provenance.elf" 2>/dev/null
fi
