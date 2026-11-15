// `import()` keeps the graph static, so it is an edge of it: reported in `lazy.js`.
const lazy = await import("./lazy.js")

export const e = lazy
