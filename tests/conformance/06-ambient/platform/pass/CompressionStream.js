// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const compressionStream = new CompressionStream("gzip")

export const use1 = new CompressionStream("a")

export const use2 = compressionStream.readable

export const use3 = compressionStream.writable
