// expect: E133 removed-global-or-member @5:26
// `substr` is removed: `slice` and `substring` do what it did.
const name = "strict"

export const tail = name.substr(1)
