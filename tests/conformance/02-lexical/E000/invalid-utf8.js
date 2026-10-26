// expect: E000 syntax @3:12
// The string below holds a lone 0xFF byte, which is not UTF-8.
const a = "ÿ"
