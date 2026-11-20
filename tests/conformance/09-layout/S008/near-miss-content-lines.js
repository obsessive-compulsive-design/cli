// Whitespace inside a template literal is content: no diagnostic.
import { f } from "./names.js"

const t = `a  

  b  
`

f(t)
