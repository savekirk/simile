import type { SetupDescription } from "../../../crates/simile-wasm/web/contract"
/** Validate scalar controls without round-tripping payload integers through Number. */
export function setupValues(
  fields: SetupDescription["collects"],
  values: Record<string, string>
): string {
  const pairs: string[] = []
  for (const field of fields) {
    const value = values[field.name]
    if (!value) {
      if (field.required) throw new Error(`${field.name} is required.`)
      continue
    }
    let literal = value
    if (field.valueType === "integer") {
      if (!/^-?(0|[1-9]\d*)$/.test(value))
        throw new Error(`${field.name} must be an integer.`)
    } else if (field.valueType === "boolean") {
      if (!["true", "false"].includes(value))
        throw new Error(`${field.name} must be true or false.`)
    } else if (field.valueType === "json") {
      JSON.parse(value) // Syntax validation only. Keep the exact original text below.
    } else literal = JSON.stringify(value)
    pairs.push(`${JSON.stringify(field.name)}:${literal}`)
  }
  return `{${pairs.join(",")}}`
}
