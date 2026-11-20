// expect: E133 removed-global-or-member @5:26
// expect: E133 removed-global-or-member @7:26
// Generated from ambient/globals.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test ambient
export const use1 = Date.parse

export const use2 = Date.notKept
