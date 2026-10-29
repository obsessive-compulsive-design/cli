// expect: E000 syntax @4:24
// `import(…)` takes a string literal path, so the module graph is static.
const name = "./x.js"
const m = await import(name)
