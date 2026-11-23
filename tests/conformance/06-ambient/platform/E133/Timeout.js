// expect: E133 removed-global-or-member @6:29
// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const timeout = setTimeout(() => undefined)

export const use1 = timeout.notKept
