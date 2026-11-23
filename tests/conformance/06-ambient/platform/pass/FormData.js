// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const formData = new FormData()

export const use1 = new FormData()

export const use2 = formData.append("a", "a")

export const use3 = formData.delete("a")

export const use4 = formData.get("a")

export const use5 = formData.getAll("a")

export const use6 = formData.has("a")

export const use7 = formData.set("a", "a")

export const use8 = formData.entries()

export const use9 = formData.keys()

export const use10 = formData.values()

export const use11 = formData.forEach(() => undefined)

export const use12 = [...formData]
