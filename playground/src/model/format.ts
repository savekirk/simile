/** Format source without round-tripping payload numbers through JavaScript values. */
export function fileType(path: string) {
  if (path.endsWith("Simfile") || path.endsWith(".hcl")) return "hcl"
  if (/\.ya?ml$/.test(path)) return "yaml"
  if (/\.[cm]?tsx?$/.test(path)) return "typescript"
  if (path.endsWith(".json")) return "json"
  if (/\.(md|mdx)$/.test(path)) return "markdown"
  return undefined
}

/** Reindent HCL only: preserve comments, strings, inline blocks, and heredoc bytes. */
export function indentHcl(source: string): string {
  let depth = 0,
    comment = false,
    heredoc: string | undefined
  const lines = source.replace(/\r\n/g, "\n").split("\n")
  const formatted = lines.map((line) => {
    if (heredoc) {
      if (line.trim() === heredoc) heredoc = undefined
      return line
    }
    const trimmed = line.trimStart()
    if (comment) {
      const end = line.indexOf("*/")
      if (end < 0) return line
      // Leave block-comment lines intact, including any following HCL.
    }
    const wasComment = comment
    let quoted = false,
      opens = 0,
      closes = 0
    let leadingCloses = 0,
      leading = true
    for (let i = 0; i < trimmed.length; i++) {
      const c = trimmed[i],
        next = trimmed[i + 1]
      if (comment) {
        if (c === "*" && next === "/") {
          comment = false
          i++
        }
        continue
      }
      if (quoted) {
        if (c === "\\") i++
        else if (c === '"') quoted = false
        continue
      }
      if (c === "#" || (c === "/" && next === "/")) break
      if (c === "/" && next === "*") {
        comment = true
        i++
        continue
      }
      if (c === '"') {
        quoted = true
        leading = false
        continue
      }
      if (c === "<" && next === "<") {
        const marker = trimmed.slice(i).match(/^<<-?([\w-]+)\s*$/)
        if (marker) {
          heredoc = marker[1]
          break
        }
      }
      if ("{[(".includes(c)) {
        opens++
        leading = false
      } else if ("}])".includes(c)) {
        closes++
        if (leading) leadingCloses++
      } else if (!/\s/.test(c)) leading = false
    }
    if (quoted)
      throw new Error("Unclosed HCL string. Fix it before formatting.")
    const indentation = depth - leadingCloses
    depth += opens - closes
    if (depth < 0 || indentation < 0)
      throw new Error("Unbalanced HCL delimiters. Fix them before formatting.")
    return wasComment ? line : trimmed ? "  ".repeat(indentation) + trimmed : ""
  })
  if (depth || comment || heredoc)
    throw new Error("Incomplete HCL document. Fix it before formatting.")
  return formatted.join("\n").trimEnd() + "\n"
}

export async function formatText(
  path: string,
  source: string
): Promise<string> {
  const type = fileType(path)
  if (type === "hcl") return indentHcl(source)
  if (!type) throw new Error(`Formatting is not available for ${path}.`)
  const { format } = await import("prettier/standalone")
  const plugins =
    type === "yaml"
      ? [await import("prettier/plugins/yaml")]
      : type === "markdown"
        ? [await import("prettier/plugins/markdown")]
        : [
            type === "json"
              ? await import("prettier/plugins/babel")
              : await import("prettier/plugins/typescript"),
            await import("prettier/plugins/estree"),
          ]
  return format(source, {
    parser: type,
    plugins,
    tabWidth: 2,
    printWidth: 88,
    embeddedLanguageFormatting: "off",
  })
}
