// expect: S001 single-quoted-string @4:11 "'say \"hi\"'" -> "\"say \\\"hi\\\"\""
// expect: S001 single-quoted-string @5:11 "'it\\'s'" -> "\"it's\""
// Only what the quotes force changes: a bare " gains a backslash, an escaped ' loses one.
const s = 'say "hi"'
const t = 'it\'s'
