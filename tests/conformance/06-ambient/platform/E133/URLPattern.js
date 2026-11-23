// expect: E133 removed-global-or-member @7:32
// expect: E133 removed-global-or-member @9:32
// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const uRLPattern = new URLPattern({ pathname: "/:id" })

export const use1 = uRLPattern.notKept

export const use2 = URLPattern.notKept
