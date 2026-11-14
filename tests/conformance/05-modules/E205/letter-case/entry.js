// expect: E205 unresolved-import @4:19
// The file is `lib.js`. A file system that ignores case would open it; one that does
// not would fail, so the spelling of the directory entry decides everywhere.
import { a } from "./Lib.js"

export const kept = a
