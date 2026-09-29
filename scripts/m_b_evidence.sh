#!/usr/bin/env bash
# Read-only evidence sweep for the M-B milestone: every claim in IMPLEMENTATION_PLAN §0.10
# and §4.1 is produced by this script, in one run, into artifacts/m-b-evidence.txt.
# No chain writes: the CLI runs are --dry-run only.
set -uo pipefail
OUT=/home/user/provenance/artifacts/m-b-evidence.txt
: > "$OUT"
{ echo "# generated $(date -u +%FT%TZ) by scripts/m_b_evidence.sh (read-only)"
  echo "# rustc: $(rustc --version)"
  echo
  echo "=== 1. workspace checks without the risc0 build flow ==="
} >> "$OUT"
cd /home/user/provenance
RISC0_SKIP_BUILD=1 cargo check --offline --workspace --all-targets >> "$OUT" 2>&1
echo "CHECK_EXIT=$?" >> "$OUT"

{ echo
  echo "=== 2. SDK unit + integration tests (hermetic) ==="
} >> "$OUT"
cd /home/user/provenance/sdk
cargo test --offline >> "$OUT" 2>&1
echo "SDK_TEST_EXIT=$?" >> "$OUT"

{ echo
  echo "=== 3. SDK live read (ignored test, real testnet RPC) ==="
} >> "$OUT"
cargo test --offline -- --ignored chain::tests::live_reads_the_deployed_entry --nocapture >> "$OUT" 2>&1
echo "LIVE_READ_EXIT=$?" >> "$OUT"

{ echo
  echo "=== 4. Storage REST smoke against a live gateway (expected to fail: no local :8080) ==="
} >> "$OUT"
cargo test --offline -- --ignored storage::tests::rest_smoke_against_live_gateway --nocapture >> "$OUT" 2>&1
echo "STORAGE_LIVE_EXIT=$? (non-zero is the honest current state, task #14)" >> "$OUT"

{ echo
  echo "=== 5. lezreg tests: ABI mirror + CLI contract + namespace regression ==="
} >> "$OUT"
cd /home/user/provenance/tools/lezreg
cargo test --offline >> "$OUT" 2>&1
echo "LEZREG_TEST_EXIT=$?" >> "$OUT"

{ echo
  echo "=== 6. lezbuild tests (guest graph, seed parsing, exit-code contract) ==="
} >> "$OUT"
cd /home/user/provenance/tools/lezbuild
cargo test --offline >> "$OUT" 2>&1
echo "LEZBUILD_TEST_EXIT=$?" >> "$OUT"

{ echo
  echo "=== 7. provenance_core unit tests ==="
} >> "$OUT"
cd /home/user/provenance
cargo test --offline -p provenance_core >> "$OUT" 2>&1
echo "CORE_TEST_EXIT=$?" >> "$OUT"

{ echo
  echo "=== 8. namespace demonstration, dry run only (self vs third-party subject) ==="
} >> "$OUT"
bash /home/user/provenance/scripts/dryrun_namespace.sh >> "$OUT" 2>&1

{ echo
  echo "=== 9. drift split on the real repo: exit 0 plain, exit 2 under --strict ==="
} >> "$OUT"
cd /home/user/provenance/tools/lezbuild
./target/debug/lezbuild verify > /tmp/mb-plain.txt 2>&1
echo "VERIFY_EXIT=$?" >> "$OUT"
head -5 /tmp/mb-plain.txt >> "$OUT"
./target/debug/lezbuild verify --strict > /tmp/mb-strict.txt 2>&1
echo "VERIFY_STRICT_EXIT=$?" >> "$OUT"
./target/debug/lezbuild manifest --offline --out /tmp/mb-manifest-dry.json > /tmp/mb-manifest-dry.log 2>&1
echo "MANIFEST_OFFLINE_EXIT=$? (written to /tmp only; the pinned artifacts/reproducible-build.json is untouched)" >> "$OUT"

echo
echo "wrote $OUT ($(wc -c < "$OUT") bytes)"
grep -E 'test result|EXIT=' "$OUT"
