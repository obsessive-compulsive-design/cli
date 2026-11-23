// expect: E133 removed-global-or-member @6:42
// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const readableStreamReader = new ReadableStream().getReader()

export const use1 = readableStreamReader.notKept
