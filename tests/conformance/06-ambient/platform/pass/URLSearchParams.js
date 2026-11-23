// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const uRLSearchParams = new URLSearchParams("a=1")

export const use1 = new URLSearchParams()

export const use2 = uRLSearchParams.size

export const use3 = uRLSearchParams.append("a", "a")

export const use4 = uRLSearchParams.delete("a")

export const use5 = uRLSearchParams.get("a")

export const use6 = uRLSearchParams.getAll("a")

export const use7 = uRLSearchParams.has("a")

export const use8 = uRLSearchParams.set("a", "a")

export const use9 = uRLSearchParams.sort()

export const use10 = uRLSearchParams.toString()

export const use11 = uRLSearchParams.entries()

export const use12 = uRLSearchParams.keys()

export const use13 = uRLSearchParams.values()

export const use14 = uRLSearchParams.forEach(() => undefined)

export const use15 = [...uRLSearchParams]
