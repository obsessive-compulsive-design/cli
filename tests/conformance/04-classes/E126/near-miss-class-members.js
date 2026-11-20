// A field's value, a `static` block, a method, an accessor and the arrows in them
// have `this`; so does a class declared in a method: no diagnostic.
export class Counter {
  static made = 0

  static {
    this.made = 0
  }

  count = 0

  later = () => this.count

  get twice() {
    return this.count * 2
  }

  step() {
    const add = (amount) => this.count + amount

    class Inner {
      size = 1

      grow() {
        return this.size + 1
      }
    }

    return [add, Inner, this]
  }
}
