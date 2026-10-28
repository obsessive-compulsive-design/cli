// expect: E201 foreign-import @3:26
// A `node:` URL is a platform module, outside the project.
import { readFile } from "node:fs"
