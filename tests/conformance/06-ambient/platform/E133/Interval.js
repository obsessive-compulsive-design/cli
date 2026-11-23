// expect: E133 removed-global-or-member @6:30
// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const interval = setInterval(() => undefined)

export const use1 = interval.notKept
