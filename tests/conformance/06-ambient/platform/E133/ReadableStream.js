// expect: E133 removed-global-or-member @7:36
// expect: E133 removed-global-or-member @9:36
// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const readableStream = new ReadableStream()

export const use1 = readableStream.notKept

export const use2 = ReadableStream.notKept
