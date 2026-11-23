// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const messagePort = new MessageChannel().port1

export const use1 = messagePort.postMessage(1)

export const use2 = messagePort.start()

export const use3 = messagePort.close()

export const use4 = messagePort.addEventListener("a", () => undefined)

export const use5 = messagePort.removeEventListener("a", () => undefined)
