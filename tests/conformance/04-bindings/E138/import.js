// expect: E138 type-mismatch @5:1
// An import is the other module's to change.
import { count } from "./count.js"

count += 1

export { count }
