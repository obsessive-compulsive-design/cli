// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const textEncoder = new TextEncoder()

const uint8Array = new Uint8Array(1)

export const use1 = new TextEncoder()

export const use2 = textEncoder.encoding

export const use3 = textEncoder.encode()

export const use4 = textEncoder.encodeInto("a", uint8Array)
