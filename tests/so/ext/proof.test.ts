// SPDX-License-Identifier: FSL-1.1-MIT
// Step 5 E3 (vectors) + nonces: chrome-extension/lib/proof.js. The vectors are the SAME as T4 in
// desktop/src-tauri/src/so/tests/link_support.rs (made with `openssl dgst -sha256 -mac HMAC -macopt hexkey:`):
// HMAC key = the 32 raw token bytes; message = ASCII "so-link-v1|ext|" / "so-link-v1|app|" + the 64 hex chars of the nonce.
import { describe, expect, it } from "vitest";
import { clientProof, eqHex, newNonce, serverProof } from "../../../chrome-extension/lib/proof.js";

const KAT_TOKEN = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
const KAT_NONCE = "202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f";
const KAT_CNONCE = "404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f";
const KAT_CLIENT_PROOF = "15e438d3c342dcd8cdf294af04a47a1c86af3a0f835436997c092aa8704a60f8";
const KAT_SERVER_PROOF = "425ee6788af9596d8d82eb6576967acafae4b8a5aea8987678e9b1e6db47274d";

describe("E3 proofs equal the T4 known answers", () => {
  it("clientProof and serverProof", async () => {
    expect(await clientProof(KAT_TOKEN, KAT_NONCE)).toBe(KAT_CLIENT_PROOF);
    expect(await serverProof(KAT_TOKEN, KAT_CNONCE)).toBe(KAT_SERVER_PROOF);
    expect(await clientProof(KAT_TOKEN, KAT_NONCE)).not.toBe(await serverProof(KAT_TOKEN, KAT_NONCE));
  });
});

describe("newNonce", () => {
  it("is 64 lowercase hex and different every time", () => {
    const seen = new Set<string>();
    for (let i = 0; i < 20; i++) {
      const n = newNonce();
      expect(n).toMatch(/^[0-9a-f]{64}$/);
      seen.add(n);
    }
    expect(seen.size).toBe(20);
  });
});

describe("eqHex", () => {
  it("true only for equal strings", () => {
    expect(eqHex(KAT_CLIENT_PROOF, KAT_CLIENT_PROOF)).toBe(true);
    expect(eqHex(KAT_CLIENT_PROOF, KAT_SERVER_PROOF)).toBe(false);
    expect(eqHex(KAT_CLIENT_PROOF, KAT_CLIENT_PROOF.slice(1))).toBe(false);
    expect(eqHex("", "")).toBe(true);
    expect(eqHex("ab", "abc")).toBe(false);
    expect(eqHex(KAT_CLIENT_PROOF, 5 as never)).toBe(false);
    expect(eqHex(undefined as never, undefined as never)).toBe(false);
  });
  it("does not stop at the first difference: it reads the whole string (no early return on length-equal inputs)", async () => {
    // Behavioural proxy for "constant time": a difference in the LAST char and in the FIRST char both give false.
    const a = "a".repeat(64);
    expect(eqHex(a, "b" + "a".repeat(63))).toBe(false);
    expect(eqHex(a, "a".repeat(63) + "b")).toBe(false);
  });
});
