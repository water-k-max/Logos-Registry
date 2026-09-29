import json, hashlib, urllib.request, base64, sys

RPC = "https://testnet.lez.logos.co/"
TX = "f6d82bf00142127c827ab3dc42cd31b53ea74a7a236943d93fbc1131075fe99d"
ENTRY = "Hm4Yzd2vgTCvhvPFQUzLT2xoYidRK84QYBzPLQnH8dbJ"
CLAIM = "A4nuRThP7GiUeLZZn2eRemHjZLaVjMyL6RNtveCBst7j"
PREV_ENTRY_SHA = "874aeba96fcdb9f8caf66a7d"   # snapshot before the send
PREV_CLAIM_NONCE = 0


def call(method, params=None):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method,
                       "params": params or {}}).encode()
    req = urllib.request.Request(RPC, data=body, headers={"content-type": "application/json"})
    for attempt in range(4):
        try:
            return json.loads(urllib.request.urlopen(req, timeout=40).read())
        except Exception as e:                      # noqa: BLE001 - transient TLS/timeout
            if attempt == 3:
                return {"error": {"message": "transport: %s" % e}}


def fetch(url):
    try:
        return urllib.request.urlopen(url, timeout=30).read()
    except Exception as e:                          # noqa: BLE001
        return b""


print("tip:", call("getLastBlockId").get("result"))

# 1. was the competing tx retained by the chain at all?
r = call("getTransaction", {"tx_hash": TX})
if "result" in r and r["result"] is not None:
    raw = base64.b64decode(r["result"][0])
    print("getTransaction: INCLUDED in block", r["result"][1], "size", len(raw))
    open("/home/user/e2-competing-tx.bin", "wb").write(raw)
    print("  tx sha256", hashlib.sha256(raw).hexdigest())
else:
    print("getTransaction: no result ->", json.dumps(r.get("error") or r))

expl = fetch("https://explorer.testnet.lez.logos.co/tx/" + TX)
print("explorer bytes:", len(expl))

# 2. did the entry change?
for name, acct in (("entry", ENTRY), ("competing-claimant", CLAIM)):
    acc = call("getAccount", {"account_id": acct}).get("result") or {}
    data = bytes(acc.get("data", []))
    sha = hashlib.sha256(data).hexdigest()[:24]
    print("%s: len=%d sha256=%s nonce=%s balance=%s" %
          (name, len(data), sha, acc.get("nonce"), acc.get("balance")))
    if name == "entry":
        print("  unchanged vs pre-send snapshot:", PREV_ENTRY_SHA in sha)
        print("  text:", data[:120])
    else:
        print("  claimant nonce moved from", PREV_CLAIM_NONCE, "->", acc.get("nonce"))
