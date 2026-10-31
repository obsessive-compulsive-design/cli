// expect: S006 line-too-long @3:1
// A doc comment's tag line is never broken; this one is wider than the line.
/**
 * Prose.
 *
 * @param a  xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
 */
export function f(a) {}
