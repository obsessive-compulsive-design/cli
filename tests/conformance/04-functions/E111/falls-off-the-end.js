// expect: E111 mixed-return @5:5
// With `found` false the function returns nothing at all.
export function label(found) {
  if (found) {
    return "found"
  }
}
