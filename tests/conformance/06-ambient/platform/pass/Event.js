// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const event = new Event("ready")

export const use1 = new Event("a")

export const use2 = event.type

export const use3 = event.target

export const use4 = event.currentTarget

export const use5 = event.composedPath()

export const use6 = event.eventPhase

export const use7 = event.stopPropagation()

export const use8 = event.stopImmediatePropagation()

export const use9 = event.bubbles

export const use10 = event.cancelable

export const use11 = event.preventDefault()

export const use12 = event.defaultPrevented

export const use13 = event.composed

export const use14 = event.isTrusted

export const use15 = event.timeStamp
