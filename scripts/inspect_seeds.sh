#!/usr/bin/env bash
set -uo pipefail
for d in /home/user/regcache /home/user/lez-programs/gitdb /home/user/prov-build; do
  if [ -d "$d" ]; then echo "OK   $d  $(du -sh "$d" | cut -f1)"; else echo "MISS $d"; fi
done
echo "--- regcache top level ---"
ls -1 /home/user/regcache 2>/dev/null | head
echo "--- gitdb top level (first 5) ---"
ls -1 /home/user/lez-programs/gitdb 2>/dev/null | head -5
echo "--- does the seed contain the logos db dir the recipe probes for? ---"
test -d /home/user/lez-programs/gitdb/logos-execution-zone-6bae42d7c9cadfe7 && echo "YES logos-execution-zone-6bae42d7c9cadfe7" || echo "NO"
echo "--- disk ---"
df -h / | tail -1
