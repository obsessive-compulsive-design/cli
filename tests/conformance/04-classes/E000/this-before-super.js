// expect: E000 syntax @7:5
// `this` is used before `super()` has been called.
import { Base } from "./base.js"

export class Derived extends Base {
  constructor(size) {
    this.size = size
    super()
  }
}
