import { describe, expect, it } from "vitest"
import { formatText, indentHcl } from "../src/model/format"

describe("editor formatting preserves authored values", () => {
  it("keeps large integers, null, escaped strings, and repeated array entries", async () => {
    const formatted = await formatText(
      "request.json",
      '{"id":18446744073709551615,"min":-9223372036854775808,"value":null,"headers":[["x","a"],["x","b"]]}'
    )
    expect(formatted).toContain("18446744073709551615")
    expect(formatted).toContain("-9223372036854775808")
    expect(formatted).toContain('"value": null')
    expect(formatted.match(/"x"/g)).toHaveLength(2)
    expect(await formatText("request.json", formatted)).toBe(formatted)
  })
  it("formats YAML, TypeScript, and Markdown with their own parsers", async () => {
    expect(
      await formatText(
        "api.yaml",
        "# keep this\ninfo: {title: Demo,version: 1.0.0}\n"
      )
    ).toContain("# keep this\ninfo: { title: Demo, version: 1.0.0 }")
    expect(
      await formatText(
        "api.d.ts",
        "export interface Value{readonly id:string;}"
      )
    ).toContain("readonly id: string;")
    expect(await formatText("NOTES.md", "#   Notes\n")).toBe("# Notes\n")
    await expect(formatText("bad.json", '{"body":')).rejects.toThrow()
  })
  it("reindents HCL without changing comments, strings, or heredoc contents", () => {
    const source =
      'service "demo" {\n# keep } in this comment\ndescription = "brace } and \\"quoted\\""\n/* block\n   } comment */\nopenapi { path = "./api.yaml" }\nnotes = <<EOF\n { literal payload }\nEOF\nlisten {\nport = 4242\n}\n}\n'
    const result = indentHcl(source)
    expect(result).toContain("  # keep } in this comment")
    expect(result).toContain('  openapi { path = "./api.yaml" }')
    expect(result).toContain("<<EOF\n { literal payload }\nEOF\n")
    expect(result).toContain("  listen {\n    port = 4242\n  }")
    expect(indentHcl(result)).toBe(result)
    expect(() => indentHcl('service "demo" {')).toThrow("Incomplete HCL")
  })
})
