// expect: E137 missing-property @7:42
// expect: E137 missing-property @7:57
// The `kind` of one member is any string: it tells the members apart no more, and
// each key is read from both.
export function area(flag) {
  const shape = flag ? { kind: "circle", radius: 1 } : { kind: `${flag}`, side: 2 }
  return shape.kind === "circle" ? shape.radius : shape.side
}
