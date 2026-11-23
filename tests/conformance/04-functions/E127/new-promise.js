// expect: E127 promise-executor @2:24
export const settled = new Promise((resolve) => resolve(1))
