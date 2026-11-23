// expect: E117 unreachable-code @16:3
// The `break` leaves the `switch`, not the loop: the loop never ends, and what follows
// it never runs.
export function drain(next, sink) {
  while (true) {
    const item = next()
    switch (item) {
      case 0: {
        break
      }
      default: {
        sink(item)
      }
    }
  }
  sink(0)
}
