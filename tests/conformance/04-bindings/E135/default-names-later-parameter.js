// expect: E135 use-before-declaration @3:31
// Defaults run left to right: `width` has none yet when `height` takes its default.
export const area = (height = width, width = 1) => height * width
