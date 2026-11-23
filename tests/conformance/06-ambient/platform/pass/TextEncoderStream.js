// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const textEncoderStream = new TextEncoderStream()

export const use1 = new TextEncoderStream()

export const use2 = textEncoderStream.encoding

export const use3 = textEncoderStream.readable

export const use4 = textEncoderStream.writable
