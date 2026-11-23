// expect: E138 type-mismatch @8:37
// A clone of a class instance is a plain object: it is no `Point` any more.
class Point {
  x = 0
  y = 0
}

export const copy = structuredClone(new Point())
