// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const writableStreamWriter = new WritableStream().getWriter()

export const use1 = writableStreamWriter.closed

export const use2 = writableStreamWriter.desiredSize

export const use3 = writableStreamWriter.ready

export const use4 = writableStreamWriter.abort()

export const use5 = writableStreamWriter.close()

export const use6 = writableStreamWriter.releaseLock()

export const use7 = writableStreamWriter.write()
