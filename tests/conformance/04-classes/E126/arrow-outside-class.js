// expect: E126 this-outside-class @3:27
// An arrow takes the `this` around it, and outside a class there is none.
export const name = () => this.name
