// expect: E133 removed-global-or-member @7:39
// expect: E133 removed-global-or-member @9:39
// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const compressionStream = new CompressionStream("gzip")

export const use1 = compressionStream.notKept

export const use2 = CompressionStream.notKept
