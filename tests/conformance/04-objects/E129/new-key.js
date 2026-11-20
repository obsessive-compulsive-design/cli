// expect: E129 shape-change @5:5
// A shape is fixed when the object is made: `depth` cannot be added.
export const box = { width: 1 }

box.depth = 2
