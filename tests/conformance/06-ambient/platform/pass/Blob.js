// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const blob = new Blob(["a"])

export const use1 = new Blob()

export const use2 = blob.size

export const use3 = blob.type

export const use4 = blob.slice()

export const use5 = blob.stream()

export const use6 = blob.text()

export const use7 = blob.arrayBuffer()

export const use8 = blob.bytes()
