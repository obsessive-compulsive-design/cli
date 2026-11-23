// A function declared after a `return` is hoisted, a loop with a `break` of its own
// ends, and a `try` that may throw is followed by what its `catch` goes on to: no
// diagnostic.
export function first(items, test) {
  let found = -1
  while (true) {
    found += 1
    if (found === items.length || matches(items[found])) {
      break
    }
  }
  try {
    return test(found)
  } catch {
    found = -1
  }
  return found

  function matches(item) {
    return item === 0
  }
}
