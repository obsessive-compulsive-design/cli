// expect: E126 this-outside-class @4:10
// A `function` has no `this`: what it needs is a parameter.
export function name() {
  return this.name
}
