# A second implementation of the Safe format, written from
# docs/safe-2026-09-28.md sections 4-5 rather than from src/safe, and checked
# against the vectors the Rust code froze. If both agree byte for byte, the
# format is what the document says it is — not merely what the Rust does.
#
# Not run by CI: it needs libsodium, argon2-cffi and blake3 from PyPI.
#
#   python3 -m venv /tmp/safe-venv
#   /tmp/safe-venv/bin/pip install pynacl argon2-cffi blake3
#   /tmp/safe-venv/bin/python src-tauri/src/safe/testdata/verify_vectors.py \
#       src-tauri/src/safe/testdata/vectors.json
import json, struct, sys
from argon2.low_level import hash_secret_raw, Type
from blake3 import blake3
from nacl.bindings import crypto_aead_xchacha20poly1305_ietf_encrypt as seal, crypto_aead_xchacha20poly1305_ietf_decrypt as open_

v = json.load(open(sys.argv[1])); i = v["inputs"]; h = bytes.fromhex
dk = lambda ctx, km: blake3(km, derive_key_context=ctx).digest()
C = "synabit safe 2026-09 "

P = hash_secret_raw(i["password"].encode(), h(i["kdf_salt"]), time_cost=i["kdf"]["t"], memory_cost=i["kdf"]["m_kib"],
                    parallelism=i["kdf"]["p"], hash_len=32, type=Type.ID, version=19)
auk = dk(C+"account unlock key", P + h(i["secret_key"]))
assert auk.hex() == v["auk"], "auk"
kc = dk(C+"key check", auk); assert kc.hex() == v["key_check"], "key_check"
for ctx, sub in v["subkeys_of_safe_key"].items():
    assert dk(ctx, h(i["safe_key"])).hex() == sub, ctx

# keyset.safe (§5.2)
aad = (b"SFK1" + struct.pack("<HH", 1, 0) + h(i["safe_id"]) + struct.pack("<IQ", 1, 7) + bytes([1])
       + struct.pack("<II", i["kdf"]["m_kib"], i["kdf"]["t"]) + bytes([i["kdf"]["p"]]) + h(i["kdf_salt"]) + kc)
assert len(aad) == 110
ks = aad + h(i["wrap_nonce"]) + seal(h(i["safe_key"]), aad, h(i["wrap_nonce"]), dk(C+"wrap safe key", auk))
assert ks.hex() == v["keyset_safe"], "keyset"

# items/<id>.safe (§5.3, §5.5)
def item(tomb, pt):
    hdr = b"SFI1" + struct.pack("<HH", 1, 1 if tomb else 0) + h(i["safe_id"]) + h(i["item_id"]) + struct.pack("<QI", 3, 1) + bytes(4)
    kek = dk(C+"wrap item key", h(i["safe_key"]))
    wik = seal(h(i["item_key"]), hdr, h(i["item_key_nonce"]), kek)
    if tomb: body = b""
    else:
        body = struct.pack("<I", len(pt)) + pt
        total = max(512, -(-len(body)//256)*256); body += bytes(total-len(body))
    return hdr + h(i["item_key_nonce"]) + wik + h(i["body_nonce"]) + seal(body, hdr, h(i["body_nonce"]), h(i["item_key"]))
assert item(False, i["plaintext"].encode()).hex() == v["item_safe"], "item"
assert item(True, b"").hex() == v["tombstone_safe"], "tombstone"

# and the Rust-written item opens here
raw = h(v["item_safe"]); hdr = raw[:56]
ik = open_(raw[80:128], hdr, raw[56:80], dk(C+"wrap item key", h(i["safe_key"])))
pt = open_(raw[152:], hdr, raw[128:152], ik); n = struct.unpack("<I", pt[:4])[0]
assert pt[4:4+n] == i["plaintext"].encode()
print("all vectors match an independent implementation")
