// expect: E133 removed-global-or-member @7:29
// expect: E133 removed-global-or-member @9:29
// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const request = new Request("https://example.com/")

export const use1 = request.notKept

export const use2 = Request.notKept
