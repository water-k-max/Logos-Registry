# Reclaim post-check: prove the destructive cleanup did not break the reproduction path.
#
# Why this exists rather than a "df -h" line: the reclaim deleted 8.6 GB of host cargo target
# output and 20 GB of BuildKit cache, and the only claim that matters afterwards is that a cold,
# sealed, network-free guest build still reproduces the DEPLOYED ImageID. That is exactly what
# scripts/verify_from_the_outside.sh tests, so the post-check is a run of the real verifier, not a
# bespoke smoke test.
#
# LAUNCH HONESTY NOTE, measured 2026-09-29: the first post-reclaim run was launched with
# `bash -c` + `setsid nohup env ...` and died at the chain leg with
# `cargo: command not found` -> "ENV FAIL: explorer-resolve exited 127" (exit 3). That was NOT a
# regression from the cleanup: `bash -c` is a non-login shell, so it never sourced the profile line
# rustup installs (~/.cargo/bin on PATH). The two legs the cleanup could actually have broken --
# bundle sha, extracted cache file counts, builder image rebuild, cold sealed build, ELF sha and
# ImageID == manifest -- had all already passed in that same run. This script therefore launches
# with `bash -lc`, which is how every earlier verifier run was launched.
set -uo pipefail
cd /home/user/provenance
LOG=artifacts/outside-postreclaim.log
rm -f "$LOG"
# The exit code is echoed INSIDE the `bash -lc` string on purpose. Asking Git-Bash to interpolate
# `$?` after `wsl.exe -d ... bash -lc "..."` has already produced a wrong reading twice this project
# (an empty `rc`, and a STRICT_EXIT=0 for a command that really exited 2): the outer shell expands
# the variable before the inner one ever runs. Inside single quotes the inner shell owns it.
setsid nohup bash -lc 'cd /home/user/provenance && TREE_CHECK= BUILD=1 BUNDLE=/home/user/prov-seeds/prov-seeds.tar.gz BASE_TAG=r0.1.88.0-provseed-bundle1 bash scripts/verify_from_the_outside.sh; rc=$?; echo "verifier exit=$rc at $(date -Is)"' > "$LOG" 2>&1 < /dev/null &
echo "launched pid=$! log=$LOG"
