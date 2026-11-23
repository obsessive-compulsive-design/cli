// expect: E118 async-no-await @3:21
// Nothing in the body is awaited: `async` only wraps the result in a promise.
export const size = async (text) => text.length
