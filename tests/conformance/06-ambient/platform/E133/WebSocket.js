// expect: E133 removed-global-or-member @7:31
// expect: E133 removed-global-or-member @9:31
// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const webSocket = new WebSocket("wss://example.com/")

export const use1 = webSocket.notKept

export const use2 = WebSocket.notKept
