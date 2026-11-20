// expect: E126 this-outside-class @10:14
// A `function` declared in a method does not have the method's `this`.
export class Counter {
  count = 0

  step() {
    return next

    function next() {
      return this.count + 1
    }
  }
}
