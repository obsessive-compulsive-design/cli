// expect: E119 await-non-promise @5:41
// `count` is a number: there is nothing to wait for.
const count = 3

export const waited = async () => await count
