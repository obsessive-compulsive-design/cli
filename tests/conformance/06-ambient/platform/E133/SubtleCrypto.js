// expect: E133 removed-global-or-member @6:34
// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const subtleCrypto = crypto.subtle

export const use1 = subtleCrypto.notKept
