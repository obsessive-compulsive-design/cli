// expect: E100 null-produced @4:10
// Returning `null` hands it to every caller.
export function find() {
  return null
}
