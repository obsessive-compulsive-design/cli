// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const messageChannel = new MessageChannel()

export const use1 = new MessageChannel()

export const use2 = messageChannel.port1

export const use3 = messageChannel.port2
