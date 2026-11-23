// expect: E113 floating-promise @7:27
// `log` does nothing with what it is passed, so the promise is dropped.
const load = async () => await Promise.resolve(1)

const log = (_value) => "logged"

export const logged = log(load())
