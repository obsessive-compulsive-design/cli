// A promise awaited, returned, stored and awaited later, or passed to `Promise.all`
// is used: no diagnostic.
const load = async () => await Promise.resolve(1)

export const stored = load()

export const later = async () => await stored

export const passedOn = () => load()

export const both = await Promise.all([load(), load()])
