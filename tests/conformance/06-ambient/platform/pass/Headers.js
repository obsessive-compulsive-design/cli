// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const headers = new Headers()

export const use1 = new Headers()

export const use2 = headers.append("a", "a")

export const use3 = headers.delete("a")

export const use4 = headers.get("a")

export const use5 = headers.getSetCookie()

export const use6 = headers.has("a")

export const use7 = headers.set("a", "a")

export const use8 = headers.entries()

export const use9 = headers.keys()

export const use10 = headers.values()

export const use11 = headers.forEach(() => undefined)

export const use12 = [...headers]
