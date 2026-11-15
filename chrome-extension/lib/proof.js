// SPDX-License-Identifier: FSL-1.1-MIT
// HMAC-SHA256 proofs (WebCrypto) and nonces. The HMAC key is the 32 raw token bytes; the message is a
// fixed prefix plus the 64 hex characters of the nonce as text. Same vectors as the app (test T4).

const hex = (bytes) => Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
const unhex = (s) => Uint8Array.from(s.match(/../g) ?? [], (h) => parseInt(h, 16));

async function mac(tokenHex, prefix, nonceHex) {
  const key = await crypto.subtle.importKey("raw", unhex(tokenHex), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  const sig = await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(prefix + nonceHex));
  return hex(new Uint8Array(sig));
}

/** HMAC(token, "so-link-v1|ext|" + nonce) as lowercase hex. */
export function clientProof(tokenHex, nonceHex) {
  return mac(tokenHex, "so-link-v1|ext|", nonceHex);
}
/** HMAC(token, "so-link-v1|app|" + cnonce) as lowercase hex. */
export function serverProof(tokenHex, cnonceHex) {
  return mac(tokenHex, "so-link-v1|app|", cnonceHex);
}
/** 32 random bytes as 64 lowercase hex. */
export function newNonce() {
  return hex(crypto.getRandomValues(new Uint8Array(32)));
}
/** Constant-time string compare: always reads the longer string to the end. */
export function eqHex(a, b) {
  if (typeof a !== "string" || typeof b !== "string") return false;
  let diff = a.length ^ b.length;
  const n = Math.max(a.length, b.length);
  for (let i = 0; i < n; i++) diff |= (a.charCodeAt(i) || 0) ^ (b.charCodeAt(i) || 0);
  return diff === 0;
}
