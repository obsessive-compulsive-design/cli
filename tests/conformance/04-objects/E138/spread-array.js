// expect: E138 type-mismatch @3:22
// An array does not spread into an object literal.
export const box = { ...[1] }
