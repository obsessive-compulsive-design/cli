// expect: E126 this-outside-class @6:16
// A parameter's default may not refer to `this`, in a method either.
export class Counter {
  count = 0

  add(amount = this.count) {
    this.count += amount
  }
}
