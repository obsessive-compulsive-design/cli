// `super()` first, then `this`; a class without `extends` calls none: no diagnostic.
import { Base } from "./base.js"

export class Derived extends Base {
  constructor(size) {
    super()
    this.size = size
  }
}

export class Plain {
  constructor(size) {
    this.size = size
  }
}
