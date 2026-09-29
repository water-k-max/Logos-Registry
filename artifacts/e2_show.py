import json, hashlib, sys

d = json.load(open(sys.argv[1]))
if "error" in d:
    print("  error:", d["error"])
    raise SystemExit
r = d.get("result") or {}
data = bytes(r.get("data", []))
print("  len=%d sha256=%s nonce=%s balance=%s owner=%s" % (
    len(data), hashlib.sha256(data).hexdigest()[:24], r.get("nonce"),
    r.get("balance"), r.get("program_owner")))
