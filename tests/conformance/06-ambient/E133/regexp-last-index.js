// expect: E133 removed-global-or-member @5:8
// A regex is not rewound by hand: `matchAll` walks every match.
const digits = /\d+/gv

digits.lastIndex = 0

export { digits }
