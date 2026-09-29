import json, urllib.request

RPC = "https://testnet.lez.logos.co/"


def call(method, params):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    req = urllib.request.Request(RPC, data=body, headers={"content-type": "application/json"})
    return json.loads(urllib.request.urlopen(req, timeout=25).read())


for a in ["8tWS2X8e59Q4FYUUExxxFzpNjbQirzkFjDjYSukVzsqe",
          "A4nuRThP7GiUeLZZn2eRemHjZLaVjMyL6RNtveCBst7j"]:
    r = call("getAccount", {"account_id": a})["result"]
    print("%s balance=%s nonce=%s" % (a, r.get("balance"), r.get("nonce")))
