// expect: E128 then-chain @5:28
// A promise is awaited, not chained.
const load = async () => await Promise.resolve(1)

export const next = load().then((value) => value + 1)
