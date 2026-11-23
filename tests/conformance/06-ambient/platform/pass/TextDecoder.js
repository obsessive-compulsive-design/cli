// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const textDecoder = new TextDecoder()

export const use1 = new TextDecoder()

export const use2 = textDecoder.encoding

export const use3 = textDecoder.fatal

export const use4 = textDecoder.ignoreBOM

export const use5 = textDecoder.decode()
