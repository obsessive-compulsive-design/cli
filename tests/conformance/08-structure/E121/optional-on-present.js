// expect: E121 constant-condition @5:25
// `box` is never `undefined` or `null`: `?.` asks a question with one answer.
const box = { width: 1 }

export const width = box?.width
