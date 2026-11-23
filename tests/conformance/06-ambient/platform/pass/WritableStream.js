// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const writableStream = new WritableStream()

export const use1 = new WritableStream()

export const use2 = writableStream.locked

export const use3 = writableStream.abort()

export const use4 = writableStream.close()

export const use5 = writableStream.getWriter()
