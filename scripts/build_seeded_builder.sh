#!/usr/bin/env bash
# Build the sealed builder image: official risc0 guest-builder + both offline cargo caches.
# One addressable artifact pins toolchain AND dependency set, so `image_digest` in
# reproducible-build.json stops being decorative.
#
#   scripts/build_seeded_builder.sh            # tag = r0.1.88.0-provseed-<date>
#   IMG_TAG=... scripts/build_seeded_builder.sh
#
# Also tags the image as risczero/risc0-guest-builder:<tag>, which is how docker/build-guest.Dockerfile
# is pointed at it: that Dockerfile's FROM line takes only a tag, and BuildKit resolves a locally
# tagged image before trying the registry (no --pull anywhere in this pipeline).
#
# The staging context is hardlinked (cp -al), not copied: the two seeds are 252 MB + 389 MB on the
# same filesystem, so a copy would be pure waste.
set -euo pipefail

SRC="${SRC:-/home/user/provenance}"
GITDB="${GITDB:-/home/user/lez-programs/gitdb}"
REGCACHE="${REGCACHE:-/home/user/regcache}"
CTX="${CTX:-/home/user/prov-builder-ctx}"
BASE="${BASE:-r0.1.88.0}"
IMG_TAG="${IMG_TAG:-$BASE-provseed-$(date +%Y%m%d)}"
IMAGE="provenance-builder-seeded:$IMG_TAG"
ALIAS="risczero/risc0-guest-builder:$IMG_TAG"

test -d "$GITDB/logos-execution-zone-6bae42d7c9cadfe7" || { echo "ABORT: git seed incomplete"; exit 2; }
test -d "$REGCACHE/cache" && test -d "$REGCACHE/index" || { echo "ABORT: registry seed incomplete"; exit 2; }
docker info >/dev/null || { echo "ABORT: docker daemon not reachable"; exit 2; }

echo "==> staging $CTX (hardlinks)"
rm -rf "$CTX"
mkdir -p "$CTX"
cp -al "$GITDB" "$CTX/gitdb"
cp -al "$REGCACHE" "$CTX/regcache"
printf 'gitdb\t%s\nregcache\t%s\n' "$(find "$CTX/gitdb" -type f | wc -l)" "$(find "$CTX/regcache" -type f | wc -l)"

echo "==> building $IMAGE"
DOCKER_BUILDKIT=1 docker build \
  -f "$SRC/docker/builder-seeded.Dockerfile" \
  --build-arg "RISC0_DOCKER_CONTAINER_TAG=$BASE" \
  -t "$IMAGE" \
  "$CTX" > "$SRC/artifacts/builder-seeded.log" 2>&1 || {
    echo "DOCKER FAILED — tail:"; tail -30 "$SRC/artifacts/builder-seeded.log"; exit 1; }

docker tag "$IMAGE" "$ALIAS"

echo "==> measured image facts"
echo "IMAGE        $IMAGE"
echo "ALIAS        $ALIAS  (use as BASE_TAG for docker/build-guest.Dockerfile)"
echo "IMAGEID      $(docker image inspect --format '{{.Id}}' "$IMAGE")"
echo "SIZE_BYTES   $(docker image inspect --format '{{.Size}}' "$IMAGE")"
echo "BASE         $(docker image inspect --format '{{.Id}}' "risczero/risc0-guest-builder:$BASE")"
echo "OFFLINE_ENV  $(docker image inspect --format '{{json .Config.Env}}' "$IMAGE" | tr ',' '\n' | grep -i offline || echo '<none>')"
echo
echo "Next: CACHE_ID=fresh-$(date +%s) SEEDED=1 BASE_TAG=$IMG_TAG scripts/build_guest.sh"
