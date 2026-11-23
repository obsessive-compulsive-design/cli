// expect: E133 removed-global-or-member @5:30
// Without a comparator `toSorted` orders by text: 10 comes before 9.
const sizes = [10, 9, 1]

export const ordered = sizes.toSorted()
