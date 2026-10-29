// expect: E000 syntax @3:16
// Elision in an array literal is removed: arrays are dense.
const xs = [1, , 2]
