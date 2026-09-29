# The sealed builder: the official cargo-risczero guest-builder image WITH both offline
# cargo caches baked in, so that pinning this image pins the registry and the git DB too.
#
# Why this exists (measured, IMPLEMENTATION_PLAN §3.5): `Cargo.lock` + the pinned base image
# are not a complete input set. A cold build with empty cache mounts has to reach crates.io and
# dies on `[28] Timeout was reached`. The two caches can travel either in the build context
# (docker/build-guest.Dockerfile + REGCACHE/GITDB) or here, in the image — the image route means
# one addressable artifact, and the `image_digest` in reproducible-build.json stops being
# decorative: it then pins the dependency set as well as the toolchain.
#
# Nothing is compiled here, so this file adds no build input of its own: the guest still builds
# from docker/build-guest.Dockerfile, which gains one stage-into-/src block so an image-borne seed
# and a context-borne seed reach the cache mounts through the same code path (see the note on
# cache-mount shadowing below).
ARG RISC0_DOCKER_CONTAINER_TAG=r0.1.88.0
FROM risczero/risc0-guest-builder:${RISC0_DOCKER_CONTAINER_TAG}

# Contents, not the wrapping directory: these are the same trees the context seed copies
# (`cp -a /src/gitdb/. /root/.cargo/git/db/`, `cp -a /src/regcache/. /root/.cargo/registry/`).
#
# NOT at /root/.cargo: measured (artifacts/verify-image-only/build.log, 2026-09-28) that baking
# them there does nothing, because the recipe mounts BuildKit `--mount=type=cache` at
# /root/.cargo/git and /root/.cargo/registry, and a cache mount SHADOWS whatever the image had at
# that path — the first cold run saw `cp: cannot stat '/src/gitdb/.'`, i.e. the guard found an
# empty fresh cache over the baked db. So the image carries a read-only seed at its own address,
# and the recipe copies it into the cache mount, exactly as it copies a context seed.
COPY gitdb/. /opt/prov-seed/gitdb/
COPY regcache/. /opt/prov-seed/regcache/

# A stranger using this image must not be able to quietly depend on the network: if a crate is
# missing the build fails with "no matching package" instead of silently downloading a different
# dependency set.
ENV CARGO_NET_OFFLINE=true
