import { jsonLanguage } from "@codemirror/lang-json"

/** Inspect syntax, never parse payload numbers into JavaScript values. */
function properties(source: string) {
  const tree = jsonLanguage.parser.parse(source)
  let valid = true
  tree.iterate({
    enter: (node) => {
      if (node.type.isError) valid = false
    },
  })
  if (!valid) return undefined
  return tree.topNode
    .getChild("Object")
    ?.getChildren("Property")
    .map((node) => {
      const key = node.getChild("PropertyName")!
      return {
        from: node.from,
        to: node.to,
        body: JSON.parse(source.slice(key.from, key.to)) === "body",
      }
    })
}

export function hasRequestBody(source: string): boolean {
  return properties(source)?.some((property) => property.body) ?? false
}

/** Remove only top-level body properties and adjacent commas, preserving other bytes. */
export function withoutRequestBody(source: string): string {
  const fields = properties(source)
  if (!fields) throw new Error("Fix the request JSON before removing its body.")
  let result = "",
    end = 0
  for (let index = 0; index < fields.length; index++) {
    if (!fields[index].body) continue
    let next = index + 1
    while (next < fields.length && fields[next].body) next++
    const from =
      next === fields.length && index > 0
        ? fields[index - 1].to
        : fields[index].from
    result += source.slice(end, from)
    end = next < fields.length ? fields[next].from : fields[next - 1].to
    index = next - 1
  }
  return result + source.slice(end)
}
