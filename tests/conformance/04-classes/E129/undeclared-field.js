// expect: E129 shape-change @6:10
// `height` is not a field of `Box`: the shape of an instance is its class.
export class Box {
  width = 0
  grow() {
    this.height = 1
  }
}
