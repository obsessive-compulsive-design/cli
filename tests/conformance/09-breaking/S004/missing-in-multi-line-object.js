// expect: S004 trailing-comma @5:7 "" -> ","
// An object with a line break after its `{` stays one key per line, so it ends in a comma.
const o = {
  a: 1,
  b: 2
}
