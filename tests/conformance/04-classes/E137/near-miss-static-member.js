// Static members are read from the class, inherited ones too; a private method is
// called on `this`: no diagnostic.
export class Counter {
  static count = 0
  static bump() {
    return Counter.count + 1
  }
  next() {
    return this.#step() + Counter.count
  }
  #step() {
    return 1
  }
}

export class Named extends Counter {}

export const bumped = [Named.bump(), new Named().next()]
