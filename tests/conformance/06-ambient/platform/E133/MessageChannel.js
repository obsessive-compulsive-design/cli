// expect: E133 removed-global-or-member @7:36
// expect: E133 removed-global-or-member @9:36
// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const messageChannel = new MessageChannel()

export const use1 = messageChannel.notKept

export const use2 = MessageChannel.notKept
