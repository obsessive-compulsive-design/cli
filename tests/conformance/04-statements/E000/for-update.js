// expect: E000 syntax @3:24
// The update is `i += k`, `i -= k`, `i++`, `i--`, `++i` or `--i`.
for (let i = 0; i < 3; i = i + 1) {}
