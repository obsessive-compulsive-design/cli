// expect: E114 uninitialized-field @5:3
// `total` has no initializer and only a method assigns it: until `add` runs it holds
// nothing.
export class Tally {
  total
  add(amount) {
    this.total = amount
  }
}
