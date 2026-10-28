// expect: E000 syntax @4:10
// A name is exported once (an ES2025 early error).
export const a = 1
export { a }
