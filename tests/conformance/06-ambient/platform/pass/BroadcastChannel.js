// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const broadcastChannel = new BroadcastChannel("news")

export const use1 = new BroadcastChannel("a")

export const use2 = broadcastChannel.name

export const use3 = broadcastChannel.postMessage(1)

export const use4 = broadcastChannel.close()

export const use5 = broadcastChannel.addEventListener("a", () => undefined)

export const use6 = broadcastChannel.removeEventListener("a", () => undefined)
