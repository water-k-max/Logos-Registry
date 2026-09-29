#!/usr/bin/env bash
set -uo pipefail
A=/home/user/provenance/artifacts
# the canonical raw entry bytes (395 B) come from the post-E2 read; the file previously
# labelled "entry-account.json" is actually the EMPTY pre-inclusion read.
cp -f "$A/e2-entry-after.bin" "$A/entry-395.bin"
mv -f "$A/entry-account.json" "$A/entry-account-EMPTY-pre-inclusion.json"
mv -f /home/user/entry-account.json /home/user/entry-account-EMPTY-pre-inclusion.json 2>/dev/null || true
sha256sum "$A/entry-395.bin" "$A/entry-account-EMPTY-pre-inclusion.json" "$A/register-tx.bin"
ls -la "$A" | grep -E 'entry-395|EMPTY'
# provenance note so a future reader (or verifier script) knows which file is which
cat > "$A/MANIFEST-e2.txt" <<'EOF'
E2 (duplicate-claim) evidence set, 2026-09-28

entry-395.bin                            the 395-byte registry entry account data, read pre-send AND
                                           post-send with identical sha256
                                           874aeba96fcdb9f8caf66a7d851835999cd9989ee25278fe5876bf396b5fb968
e2acct-Hm4Yzd2v...json                   full getAccount JSON for the entry PDA (pre-send snapshot)
e2acct-A4nuRThP...json                   pre-send snapshot of the attacker authority (len 0, nonce 0)
e2-entry-after.{bin,json}                post-send re-read of the same two accounts
e2-dryrun.log / e2-send.log              the 144-word payload as reviewed, then as submitted
e2-competing-txhash.txt                  f6d82bf0... -> getTransaction returns null (never included)
e2_verify.py / e2_control.py             the scripts that produced the above, incl. the control read
                                           of be829ca1... (block 28803, 877 B, sha 493c291c...)
entry-account-EMPTY-pre-inclusion.json   NOT the entry: this is the all-zero read taken seconds after
                                           the first --send, before inclusion. Renamed 2026-09-28 because
                                           it was previously mislabelled "entry-account.json".
EOF
echo "=== manifest written"; cat "$A/MANIFEST-e2.txt" | head -5
