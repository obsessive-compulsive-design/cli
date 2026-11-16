// expect: E135 use-before-declaration @3:22
// `limit` is declared on the next line: here it holds nothing yet.
export const first = limit

const limit = 1

export const second = limit
