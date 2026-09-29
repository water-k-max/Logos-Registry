#!/usr/bin/env bash
# Does the builder image actually carry the read-only seed? The first image-only run failed
# because baking into /root/.cargo/* is pointless (cache mounts shadow it), so verify the
# /opt/prov-seed content BEFORE spending 20 minutes on a cold build.
set -eu
IMG="${IMG:-risczero/risc0-guest-builder:r0.1.88.0-provseed-20260928}"
echo "IMAGE     $IMG"
docker image inspect "$IMG" --format 'IMAGEID {{.Id}}'
docker image inspect "$IMG" --format 'OFFLINE_ENV {{.Config.Env}}'
docker run --rm --entrypoint sh "$IMG" -c 'ls -d /opt/prov-seed/* 2>&1; echo "---"; du -sh /opt/prov-seed/* 2>&1; echo "---"; for d in gitdb regcache; do echo "$d files=$(find /opt/prov-seed/$d -type f 2>/dev/null | wc -l)"; done; echo "---"; ls /opt/prov-seed/gitdb | head -3; echo "---"; ls /root/.cargo 2>&1'
