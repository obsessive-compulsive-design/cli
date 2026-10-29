// JsonPath: a `./` or `../` path ending in `.json`, with the attribute.
import one from "./one.json" with { type: "json" }
import two from "../data/two.json" with { type: "json" }
one.use(two)
