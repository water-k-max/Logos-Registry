#!/usr/bin/env bash
# Pre-flight for the disk reclaim: which binaries does the reproduction path actually need, and where
# do they live? Deleting a target/ dir that holds risc0-packager or imgid would break
# scripts/build_guest.sh's defaults (it aborts with "packager ... missing"), so this is checked first
# rather than discovered by a failed freeze run.
set -uo pipefail
SRC=/home/user/provenance

echo "=== binaries the scripts call by default ==="
for b in "$SRC/target/debug/risc0-packager" "$SRC/tools/risc0-packager/target/debug/risc0-packager" \
         /home/user/imgid/target/release/imgid "$SRC/tools/lezbuild/target/debug/lezbuild" \
         "$SRC/tools/lezreg/target/debug/lezreg"; do
    if [ -x "$b" ]; then
        printf '  PRESENT  %-58s %s\n' "$b" "$(stat -c '%s B  mtime %y' "$b" | cut -d' ' -f1-6)"
    else
        printf '  absent   %s\n' "$b"
    fi
done

echo "=== every risc0-packager / imgid copy on this disk (excluding prune dirs) ==="
find /home/user -name 'risc0-packager' -type f -executable 2>/dev/null | sed 's/^/  /'
find /home/user -maxdepth 5 -name 'imgid' -type f -executable 2>/dev/null | sed 's/^/  /'

echo "=== can these be rebuilt offline? (source presence, not a claim about build success) ==="
for p in tools/risc0-packager/Cargo.toml tools/lezbuild/Cargo.toml /home/user/imgid/Cargo.toml; do
    printf '  %-40s %s\n' "$p" "$([ -f "$SRC/$p" ] && echo yes || { [ -f "$p" ] && echo yes || echo no; })"
done

echo "=== usage before ==="
df -h / | tail -1 | awk '{printf "  WSL /: used %s avail %s\n", $3, $4}'
du -sh "$SRC/target" /home/user/prov-build /home/user/prov-build-cold /home/user/prov-build-sealed \
       /home/user/prov-builder-ctx /home/user/prov-build-vendor 2>/dev/null | sed 's/^/  /'
