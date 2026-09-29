#!/usr/bin/env bash
# Exit-code contract for scripts/verify_from_the_outside.sh.
#
# Two traps this file exists to defuse, both hit for real while writing it:
#   * `bash script; echo $?` handed to `wsl.exe -d … bash -lc "…"` is mangled by the Git-Bash
#     layer and reported 0 for a run that exited 2 — so codes are compared *inside* this script.
#   * `VAR=x some_function` does not put VAR in the environment of the `bash` child the function
#     spawns, so every failure case silently ran against the real manifest and exited 0. `env`
#     takes the assignment as an argument, which is exported for real.
#
# A verifier whose failure paths exit 0 is worse than no verifier: CI reads the number, not the
# prose. Case 2 is the point of the whole exercise — the manifest moved under the chain record,
# which is exactly the stale Tier-3 claim that must not pass.
set -uo pipefail
SRC=/home/user/provenance
V="$SRC/scripts/verify_from_the_outside.sh"
TAMPERED=/tmp/prov-manifest-tampered.json
pass=0; fail=0

# expect <label> <want code> [VAR=val …] <command…>
# The command is spelled `bash "$V"` rather than "$V": re-writing the script from an editor resets
# its mode to 644, and an exit-code contract should not hinge on the executable bit.
expect() {
    local label="$1" want="$2"; shift 2
    local log="/tmp/outside-${label// /-}.log"
    env "$@" > "$log" 2>&1
    local got=$?
    if [ "$got" = "$want" ]; then
        printf 'ok    %-28s exit %s\n' "$label" "$got"; pass=$((pass+1))
    else
        printf 'FAIL  %-28s exit %s, wanted %s\n' "$label" "$got" "$want"; fail=$((fail+1))
        printf '      last lines of %s:\n' "$log"; tail -3 "$log" | sed 's/^/      | /'
    fi
}

expect "matching" 0 TREE_CHECK= bash "$V"

# a source tree hash that is not the one recorded on chain
python3 - "$SRC/artifacts/reproducible-build.json" "$TAMPERED" <<'PY'
import json, sys
m = json.load(open(sys.argv[1]))
m["source"]["tree_sha256"] = "1" * 64
json.dump(m, open(sys.argv[2], "w"), indent=2, sort_keys=True)
PY
expect "stale manifest" 2 TREE_CHECK= MANIFEST="$TAMPERED" bash "$V"

# the same tampered manifest with the tree leg enabled: the claim breaks at the tree, which is
# checked (and refused) before the chain is even read. Deterministic whatever the checkout looks
# like, because the expectation is a fake digest rather than this tree's real one.
expect "stale source tree" 4 MANIFEST="$TAMPERED" bash "$V"

# no manifest at all is an environment failure, not a verification result
expect "missing manifest" 3 MANIFEST=/nonexistent/reproducible-build.json bash "$V"

# an unparseable manifest must be refused, never compared as blanks
printf 'not json at all\n' > "$TAMPERED"
expect "manifest unparseable" 3 MANIFEST="$TAMPERED" bash "$V"

# a manifest missing one expectation must abort rather than match it against ""
python3 - "$SRC/artifacts/reproducible-build.json" "$TAMPERED" <<'PY'
import json, sys
m = json.load(open(sys.argv[1]))
del m["guest_graph"]["dep_set_sha256"]
json.dump(m, open(sys.argv[2], "w"), indent=2, sort_keys=True)
PY
expect "manifest field absent" 3 MANIFEST="$TAMPERED" bash "$V"

rm -f "$TAMPERED"
printf '\n%s passed, %s failed\n' "$pass" "$fail"
if [ "$fail" = 0 ]; then
    echo "ALL EXIT CODES CORRECT"
else
    echo "EXIT CODE CONTRACT BROKEN"
    exit 1
fi
