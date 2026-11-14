// Every path names a file: through `.` and `..`, by two spellings of one file, as JSON,
// in a re-export and in `import()`.
import { a as again } from "./lib/../lib/a.js"
import data from "./data/c.json" with { type: "json" }
import { a } from "./lib/a.js"

export { b } from "./lib/b.js"

const lazy = await import("./lazy.js")

export const kept = [data, a, again, lazy]
