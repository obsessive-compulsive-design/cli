// expect: E204 unknown-global @5:32
// expect: E204 unknown-global @7:32
// Generated from ambient/globals.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test ambient
export const use1 = Uint8Array.fromBase64

export const use2 = Uint8Array.fromHex
