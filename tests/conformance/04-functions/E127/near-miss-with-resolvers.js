// `Promise.withResolvers()` gives the promise and what settles it: no diagnostic.
const { promise, resolve } = Promise.withResolvers()

resolve(1)

export const value = await promise
