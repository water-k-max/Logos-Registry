#!/usr/bin/env bash
# Read-only probe of the sequencer JSON-RPC surface, used to pin the SDK's
# response deserializers against live shapes (no writes, no signing).
set -uo pipefail
RPC="${RPC:-https://testnet.lez.logos.co/}"

post() {
    curl -s -m 40 -X POST "$RPC" -H 'content-type: application/json' -d "$1"
    echo
}

echo "### getLastBlockId"
post '{"jsonrpc":"2.0","id":1,"method":"getLastBlockId","params":{}}'

echo "### getAccount (live entry PDA)"
post '{"jsonrpc":"2.0","id":1,"method":"getAccount","params":{"account_id":"Hm4Yzd2vgTCvhvPFQUzLT2xoYidRK84QYBzPLQnH8dbJ"}}' \
    | python3 -c 'import json,sys
d = json.load(sys.stdin)
r = d.get("result")
if r is None:
    print(json.dumps(d))
else:
    print(json.dumps({"keys": sorted(r), "program_owner": r["program_owner"],
                      "balance": r["balance"], "nonce": r["nonce"],
                      "data_len": len(r["data"]),
                      "data_sha256": __import__("hashlib").sha256(bytes(r["data"])).hexdigest()}))'

echo "### getAccount (random account that cannot exist)"
post '{"jsonrpc":"2.0","id":1,"method":"getAccount","params":{"account_id":"11111111111111111111111111111111"}}'

echo "### getTransaction (unknown hash)"
post '{"jsonrpc":"2.0","id":1,"method":"getTransaction","params":{"tx_hash":"0000000000000000000000000000000000000000000000000000000000000000"}}'

echo "### unknown method"
post '{"jsonrpc":"2.0","id":1,"method":"notAMethod","params":{}}'
