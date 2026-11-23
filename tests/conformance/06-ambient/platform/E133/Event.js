// expect: E133 removed-global-or-member @7:27
// expect: E133 removed-global-or-member @9:27
// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const event = new Event("ready")

export const use1 = event.notKept

export const use2 = Event.notKept
