// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const cryptoKey = await crypto.subtle.importKey("raw", new Uint8Array(1), "HMAC", true, ["sign"])

export const use1 = cryptoKey.type

export const use2 = cryptoKey.extractable

export const use3 = cryptoKey.algorithm

export const use4 = cryptoKey.usages
