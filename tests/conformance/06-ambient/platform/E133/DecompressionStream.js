// expect: E133 removed-global-or-member @7:41
// expect: E133 removed-global-or-member @9:41
// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const decompressionStream = new DecompressionStream("gzip")

export const use1 = decompressionStream.notKept

export const use2 = DecompressionStream.notKept
