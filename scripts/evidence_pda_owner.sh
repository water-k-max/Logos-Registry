#!/usr/bin/env bash
# Evidence: which program id owns the PDA space for `pda = [literal(..), arg(..)]`
# accounts — the executing (registry) program, or an ImageID passed as an argument?
# The answer decides how a host SDK must derive entry PDAs for third-party programs.
set -uo pipefail
S="$HOME/.cargo/git/checkouts/spel-411f019390120158/1ef0500"
L="$HOME/.cargo/git/checkouts/logos-execution-zone-6bae42d7c9cadfe7/47eba25"

echo "### spel checkout present?"
ls -d "$S" "$L" 2>&1 | head

echo
echo "### where the macro/runtime turns IDL seeds into an address"
grep -rn --include=*.rs -E "for_public_pda|compute_pda\(|PdaSeed::new|pda_seed" "$S" | grep -v "/tests/" | head -30

echo
echo "### how the runtime knows the owner program id (self id accessors)"
grep -rn --include=*.rs -E "fn (self_|current_)?program_id|program_id\(\)|ctx\.program" "$S" | grep -v "/tests/" | head -20

echo
echo "### lee_core: for_public_pda implementation"
grep -rn --include=*.rs -A 14 "pub fn for_public_pda" "$L" | head -40
