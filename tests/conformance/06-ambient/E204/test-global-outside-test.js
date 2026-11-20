// expect: E204 unknown-global @3:1
// `describe` is a global of `.test.js` files only.
describe("sum", () => {})
