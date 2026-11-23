// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const abortController = new AbortController()

export const use1 = new AbortController()

export const use2 = abortController.signal

export const use3 = abortController.abort()
