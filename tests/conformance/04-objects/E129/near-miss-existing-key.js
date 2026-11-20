// Writing a key the object has, with a value it takes: no diagnostic.
export const box = { width: 1, tag: "a" }

box.width = 2.5

box.tag = "b"

box["width"] += 1
