// expect: S011 arrow-block-return @4:17 "{ return a }" -> "a"
import { a } from "./names.js"

const f = () => { return a }

export { f }
