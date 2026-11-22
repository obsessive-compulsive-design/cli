// A callback may name fewer parameters than it is passed: no diagnostic.
const each = (f) => f("a", 0)

export const lengths = [each((item) => item.length), each(() => 0)]
