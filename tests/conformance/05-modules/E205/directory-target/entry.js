// expect: E205 unresolved-import @3:19
// `dir.js` is a directory, whatever its name says.
import { a } from "./dir.js"

export const kept = a
