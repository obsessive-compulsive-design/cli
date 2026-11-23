// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const abortSignal = new AbortController().signal

export const use1 = AbortSignal.abort()

export const use2 = AbortSignal.timeout(1)

export const use3 = AbortSignal.any([abortSignal])

export const use4 = abortSignal.aborted

export const use5 = abortSignal.reason

export const use6 = abortSignal.throwIfAborted()
