// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const file = new File(["a"], "a.txt")

export const use1 = new File([1], "a")

export const use2 = file.name

export const use3 = file.lastModified
