#!/usr/bin/env bash
# Evidence 2: which ProgramId does the *runtime* use when it validates a
# `pda = [...]` account — the executing program's own id, or an id taken from
# the instruction args? This decides how an SDK derives entry PDAs for
# programs other than the registry itself.
set -uo pipefail
S="$HOME/.cargo/git/checkouts/spel-411f019390120158/1ef0500"
L="$HOME/.cargo/git/checkouts/logos-execution-zone-6bae42d7c9cadfe7/47eba25"
G="/home/user/provenance"

echo "### generated code for our own guest (pda validation as compiled)"
find "$G/target" -name "provenance*.rs" 2>/dev/null | grep -v deps | head -10
grep -rn "for_public_pda" "$G/target" --include=*.rs 2>/dev/null | grep -v "/deps/" | head -10

echo
echo "### for_public_pda call sites in spel + LEZ runtime"
grep -rn --include=*.rs "for_public_pda" "$S" "$L" | grep -viE "/tests/|test_" | head -20

echo
echo "### PdaSeed / pda validation in the LEZ state machine"
grep -rn --include=*.rs -B4 -A12 "fn.*pda.*ProgramId|PdaSeed::new" "$L/lee/state_machine" | grep -v "^--$" | head -40

echo
echo "### does the guest crate spell out the seeds check? (our FFI crate)"
grep -rn "pda\|PdaSeed" "$G/provenance_ffi/src" 2>/dev/null | head -20
ls "$G/provenance_ffi/src" 2>/dev/null
