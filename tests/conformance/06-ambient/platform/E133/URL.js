// expect: E133 removed-global-or-member @7:25
// expect: E133 removed-global-or-member @9:25
// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const uRL = new URL("https://example.com/")

export const use1 = uRL.notKept

export const use2 = URL.notKept
