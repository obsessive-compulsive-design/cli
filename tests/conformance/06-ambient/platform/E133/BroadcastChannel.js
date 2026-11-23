// expect: E133 removed-global-or-member @7:38
// expect: E133 removed-global-or-member @9:38
// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const broadcastChannel = new BroadcastChannel("news")

export const use1 = broadcastChannel.notKept

export const use2 = BroadcastChannel.notKept
