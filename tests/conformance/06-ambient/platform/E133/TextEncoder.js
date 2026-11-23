// expect: E133 removed-global-or-member @7:33
// expect: E133 removed-global-or-member @9:33
// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const textEncoder = new TextEncoder()

export const use1 = textEncoder.notKept

export const use2 = TextEncoder.notKept
