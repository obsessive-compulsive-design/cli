// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const subtleCrypto = crypto.subtle

const cryptoKey = await crypto.subtle.importKey("raw", new Uint8Array(1), "HMAC", true, ["sign"])

export const use1 = subtleCrypto.encrypt(1, cryptoKey, 1)

export const use2 = subtleCrypto.decrypt(1, cryptoKey, 1)

export const use3 = subtleCrypto.sign(1, cryptoKey, 1)

export const use4 = subtleCrypto.verify(1, cryptoKey, 1, 1)

export const use5 = subtleCrypto.digest(1, 1)

export const use6 = subtleCrypto.generateKey(1, true, ["a"])

export const use7 = subtleCrypto.deriveKey(1, cryptoKey, 1, true, ["a"])

export const use8 = subtleCrypto.deriveBits(1, cryptoKey)

export const use9 = subtleCrypto.importKey("a", 1, 1, true, ["a"])

export const use10 = subtleCrypto.exportKey("a", cryptoKey)

export const use11 = subtleCrypto.wrapKey("a", cryptoKey, cryptoKey, 1)

export const use12 = subtleCrypto.unwrapKey("a", 1, cryptoKey, 1, 1, true, ["a"])
