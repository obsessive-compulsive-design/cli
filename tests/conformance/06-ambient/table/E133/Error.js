// expect: E133 removed-global-or-member @5:27
// expect: E133 removed-global-or-member @7:27
// Generated from ambient/globals.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test ambient
export const use1 = Error.captureStackTrace

export const use2 = Error.notKept
