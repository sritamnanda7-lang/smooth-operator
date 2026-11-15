// SPDX-License-Identifier: FSL-1.1-MIT
// HMAC-SHA256 proofs (WebCrypto) and nonces. Stubs until the tests exist.

/** HMAC(token, "so-link-v1|ext|" + nonce) as lowercase hex. */
export async function clientProof(_tokenHex, _nonceHex) {
  throw new Error("not implemented");
}
/** HMAC(token, "so-link-v1|app|" + cnonce) as lowercase hex. */
export async function serverProof(_tokenHex, _cnonceHex) {
  throw new Error("not implemented");
}
/** 32 random bytes as 64 lowercase hex. */
export function newNonce() {
  throw new Error("not implemented");
}
/** Constant-time string compare. */
export function eqHex(_a, _b) {
  throw new Error("not implemented");
}
