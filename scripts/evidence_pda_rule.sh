#!/usr/bin/env bash
# Evidence 3: the exact state-machine rule for PDA-derived accounts (who is the
# namespace program) — read in context, not by grep summary.
set -uo pipefail
L="$HOME/.cargo/git/checkouts/logos-execution-zone-6bae42d7c9cadfe7/47eba25"
F="$L/lee/privacy_preserving_circuit/src/execution_state.rs"

echo "### execution_state.rs around 380-420 (pda create path)"
sed -n '380,420p' "$F"
echo
echo "### execution_state.rs around 555-590 (pda seed ownership check)"
sed -n '555,590p' "$F"
echo
echo "### validated_state_diff/mod.rs around 220-250"
sed -n '220,250p' "$L/lee/state_machine/src/validated_state_diff/mod.rs"
echo
echo "### core/src/program/tests.rs:320-340 (what 'caller' means in the pinned test)"
sed -n '315,340p' "$L/lee/state_machine/core/src/program/tests.rs"
