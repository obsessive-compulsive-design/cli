// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const readableStreamReader = new ReadableStream().getReader()

export const use1 = readableStreamReader.closed

export const use2 = readableStreamReader.cancel()

export const use3 = readableStreamReader.read()

export const use4 = readableStreamReader.releaseLock()
