// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const webSocket = new WebSocket("wss://example.com/")

export const use1 = new WebSocket("a")

export const use2 = webSocket.url

export const use3 = webSocket.readyState

export const use4 = webSocket.bufferedAmount

export const use5 = webSocket.extensions

export const use6 = webSocket.protocol

export const use7 = webSocket.binaryType

export const use8 = webSocket.close()

export const use9 = webSocket.send("a")

export const use10 = webSocket.addEventListener("a", () => undefined)

export const use11 = webSocket.removeEventListener("a", () => undefined)
