import json, hashlib, urllib.request, base64

RPC = "https://testnet.lez.logos.co/"
ORIGINAL = "be829ca10c2dce5652a29fedc9887de45a3252bfc403c604d712a2169c023dd8"   # succeeded, block 28803
COMPETING = "f6d82bf00142127c827ab3dc42cd31b53ea74a7a236943d93fbc1131075fe99d"   # E2 duplicate claim
ENTRY = "Hm4Yzd2vgTCvhvPFQUzLT2xoYidRK84QYBzPLQnH8dbJ"


def call(method, params=None):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method,
                       "params": params or {}}).encode()
    req = urllib.request.Request(RPC, data=body, headers={"content-type": "application/json"})
    for attempt in range(4):
        try:
            return json.loads(urllib.request.urlopen(req, timeout=40).read())
        except Exception as e:                      # noqa: BLE001
            if attempt == 3:
                return {"error": {"message": "transport: %s" % e}}


print("tip:", call("getLastBlockId").get("result"))
for label, h in (("control: original register tx", ORIGINAL),
                 ("E2: competing claim tx", COMPETING)):
    r = call("getTransaction", {"tx_hash": h})
    res = r.get("result")
    if res:
        raw = base64.b64decode(res[0])
        print("%-32s -> block %s, %d bytes, sha %s" % (
            label, res[1], len(raw), hashlib.sha256(raw).hexdigest()[:16]))
    else:
        print("%-32s -> %s" % (label, json.dumps(r)))

# full entry hash, for the record
acc = call("getAccount", {"account_id": ENTRY}).get("result") or {}
data = bytes(acc.get("data", []))
print("entry len=%d sha256=%s" % (len(data), hashlib.sha256(data).hexdigest()))
open("/home/user/provenance/artifacts/e2-entry-after.json", "w").write(json.dumps(acc))
open("/home/user/provenance/artifacts/e2-entry-after.bin", "wb").write(data)
open("/home/user/provenance/artifacts/e2-competing-txhash.txt", "w").write(COMPETING + "\n")
