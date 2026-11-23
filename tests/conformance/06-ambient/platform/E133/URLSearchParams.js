// expect: E133 removed-global-or-member @7:37
// expect: E133 removed-global-or-member @9:37
// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const uRLSearchParams = new URLSearchParams("a=1")

export const use1 = uRLSearchParams.notKept

export const use2 = URLSearchParams.notKept
