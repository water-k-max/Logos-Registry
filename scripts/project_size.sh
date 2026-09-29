#!/usr/bin/env bash
# Answer to "how big is this project" in the only units that matter separately: what a reviewer
# clones, what the working tree adds, and what the build/cache directories add (all reproducible,
# none of them part of the deliverable).
set -uo pipefail
SRC=/home/user/provenance
cd "$SRC" || exit 3

echo "=== what a reviewer gets (git-tracked content) ==="
printf '  files        %s\n' "$(git ls-files | wc -l)"
# `xargs -0 -d ""` is invalid (the delimiter must be one character); -0 already means NUL-separated.
printf '  bytes        %s\n' "$(git ls-files -z | xargs -0 stat -c '%s' | awk '{t += $1} END {print t}')"
printf '  .git         %s\n' "$(du -sh .git | cut -f1)"

echo "=== working tree, by component ==="
du -sh --exclude=.git "$SRC" | sed 's/^/  whole tree   /'
for d in target artifacts tools sdk methods examples scripts docker; do
    [ -e "$d" ] && printf '  %-13s %s\n' "$d" "$(du -sh "$d" 2>/dev/null | cut -f1)"
done
echo "  --- nested build dirs (the space, not the deliverable):"
du -sh $(find "$SRC" -maxdepth 3 -type d -name target -not -path '*/node_modules/*' 2>/dev/null) 2>/dev/null \
    | sort -h | tail -8 | sed 's/^/    /'
du -sh $(find "$SRC" -maxdepth 3 -type d -name 'node_modules' 2>/dev/null) 2>/dev/null \
    | sort -h | tail -4 | sed 's/^/    /'

echo "=== the other project directories this work uses (all outside the repo) ==="
du -sh /home/user/prov-seeds /home/user/regcache /home/user/lez-programs/gitdb /home/user/imgid \
       /home/user/prov-build /home/user/prov-build-cold /home/user/prov-build-sealed \
       /home/user/prov-builder-ctx /home/user/prov-build-vendor /home/user/risc0-artifacts 2>/dev/null | sed 's/^/  /'

echo "=== filesystem headroom ==="
df -h / | tail -1 | awk '{printf "  WSL / : size %s used %s avail %s (%s)\n", $2, $3, $4, $5}'
