// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const readableStream = new ReadableStream()

const writableStream = new WritableStream()

export const use1 = new ReadableStream()

export const use2 = readableStream.locked

export const use3 = readableStream.cancel()

export const use4 = readableStream.getReader()

export const use5 = readableStream.pipeThrough(1)

export const use6 = readableStream.pipeTo(writableStream)

export const use7 = readableStream.tee()

export const use8 = readableStream.values()

export const use9 = Array.fromAsync(readableStream)
