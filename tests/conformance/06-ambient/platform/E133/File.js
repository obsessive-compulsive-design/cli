// expect: E133 removed-global-or-member @7:26
// expect: E133 removed-global-or-member @9:26
// Generated from ambient/platform/*.toml, not written by hand:
// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test platform
const file = new File(["a"], "a.txt")

export const use1 = file.notKept

export const use2 = File.notKept
