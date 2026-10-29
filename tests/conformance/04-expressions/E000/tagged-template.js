// expect: E000 syntax @4:15
// Tagged templates other than `String.raw` are removed (SPEC.md §12).
const x = 1
const s = html`<p>${x}</p>`
