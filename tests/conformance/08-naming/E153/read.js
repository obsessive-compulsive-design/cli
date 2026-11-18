// expect: E153 underscore-prefix @3:28
// `_scale` says nothing reads it, and the body does.
export const area = (size, _scale) => size * _scale
