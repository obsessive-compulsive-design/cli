// expect: E111 mixed-return @5:5
// One path returns a value and another returns none.
export function label(found) {
  if (found) {
    return
  }
  return "missing"
}
