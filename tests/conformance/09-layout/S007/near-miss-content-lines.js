// A line inside a template literal or a block comment is content, not indentation.
import { f } from "./names.js"

const t = `
   three spaces
`

/*
   three spaces
 */
f(t)
