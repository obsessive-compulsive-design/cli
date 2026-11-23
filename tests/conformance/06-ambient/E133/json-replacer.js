// expect: E133 removed-global-or-member @5:44
// A replacer rewrites the value as it is written: shape it first.
const config = { name: "strict", secret: "x" }

export const text = JSON.stringify(config, ["name"])
