// expect: E137 missing-property @10:30
// `count` is a static member, and `total` is none.
export class Counter {
  static count = 0
  static bump() {
    return Counter.count + 1
  }
}

export const total = Counter.total
