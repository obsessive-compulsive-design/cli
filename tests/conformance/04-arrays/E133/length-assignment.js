// expect: E133 removed-global-or-member @5:7
// The length of an array is what is in it: `splice` takes elements out.
export const names = ["a", "b"]

names.length = 0
