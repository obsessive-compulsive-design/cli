// expect: E133 removed-global-or-member @7:33
// expect: E133 removed-global-or-member @9:33
// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const customEvent = new CustomEvent("ready", { detail: 1 })

export const use1 = customEvent.notKept

export const use2 = CustomEvent.notKept
