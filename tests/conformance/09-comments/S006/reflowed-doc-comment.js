// expect: S006 line-too-long @2:1
// expect: S006 line-too-long @4:1 "/**\n * word word word word word word word word word word word word word word word word word word word word word word word word word\n */" -> "/**\n * word word word word word word word word word word word word word word word word word word word\n * word word word word word word\n */"
// A doc comment line over the width: S006 with the reflow as its replacement.
/**
 * word word word word word word word word word word word word word word word word word word word word word word word word word
 */
export function f() {}
