// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const eventTarget = new EventTarget()

const event = new Event("ready")

export const use1 = new EventTarget()

export const use2 = eventTarget.addEventListener("a", () => undefined)

export const use3 = eventTarget.removeEventListener("a", () => undefined)

export const use4 = eventTarget.dispatchEvent(event)
