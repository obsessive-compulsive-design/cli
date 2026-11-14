// expect: E205 unresolved-import @3:22
// No file is at the path: what the program is made of is on disk.
import { gone } from "./gone.js"

export const kept = gone
