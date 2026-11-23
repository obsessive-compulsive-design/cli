// expect: E114 uninitialized-field @5:3
// The loop may run no round at all: on that path the constructor never assigns
// `last`.
export class Trail {
  last
  constructor(points) {
    for (const point of points) {
      this.last = point
    }
  }
}
