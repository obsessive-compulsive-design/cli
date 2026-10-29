// expect: E000 syntax @3:24
// A template is not a string literal, even without substitutions.
const m = await import(`./x.js`)
