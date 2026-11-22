// expect: E114 uninitialized-field @4:3
// The constructor assigns `label` on one path only.
export class Tag {
  label
  constructor(label) {
    if (label !== "") {
      this.label = label
    }
  }
}
