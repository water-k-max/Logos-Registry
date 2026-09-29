#!/usr/bin/env bash
set -uo pipefail
A=/home/user/provenance/artifacts
mkdir -p "$A"
for f in /home/user/e2-send.log /home/user/e2-dryrun.log /home/user/e2acct-*.json /home/user/e2_snapshot.sh /home/user/e2_bal.sh /home/user/e2_show.py /home/user/e2_verify.py /home/user/e2_control.py /home/user/e2_balance.py /home/user/e2_txsrc.sh; do
  [ -e "$f" ] && cp -f "$f" "$A/" && echo "copied $(basename "$f")"
done
echo "=== provenance/artifacts (e2 + register)"
ls -la "$A" | grep -Ei 'e2|entry|register' 
echo "=== sha256 of the protected entry, before-vs-after record"
sha256sum "$A/e2-entry-after.bin" "$A"/entry-*.bin 2>/dev/null || sha256sum "$A/e2-entry-after.bin"
