// expect: E000 syntax @4:8
// `undefined` is a name, not a literal: narrow before the switch.
switch (x) {
  case undefined: {}
}
