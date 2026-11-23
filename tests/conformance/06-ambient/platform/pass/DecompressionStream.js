// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const decompressionStream = new DecompressionStream("gzip")

export const use1 = new DecompressionStream("a")

export const use2 = decompressionStream.readable

export const use3 = decompressionStream.writable
