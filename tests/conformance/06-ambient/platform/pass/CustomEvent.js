// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const customEvent = new CustomEvent("ready", { detail: 1 })

export const use1 = new CustomEvent("a")

export const use2 = customEvent.detail
