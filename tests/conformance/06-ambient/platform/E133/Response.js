// expect: E133 removed-global-or-member @7:30
// expect: E133 removed-global-or-member @9:30
// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const response = new Response("body")

export const use1 = response.notKept

export const use2 = Response.notKept
