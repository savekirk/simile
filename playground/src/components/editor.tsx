import { useEffect, useRef } from "react"
import { EditorState } from "@codemirror/state"
import {
  EditorView,
  keymap,
  lineNumbers,
  highlightActiveLineGutter,
} from "@codemirror/view"
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands"
import {
  syntaxHighlighting,
  HighlightStyle,
  StreamLanguage,
} from "@codemirror/language"
import { yaml } from "@codemirror/lang-yaml"
import { json } from "@codemirror/lang-json"
import { javascript } from "@codemirror/lang-javascript"
import { tags } from "@lezer/highlight"
const highlighting = HighlightStyle.define([
  {
    tag: [tags.keyword, tags.propertyName, tags.labelName],
    color: "var(--syntax-keyword)",
  },
  { tag: [tags.string, tags.regexp], color: "var(--syntax-string)" },
  { tag: [tags.number, tags.bool, tags.null], color: "var(--syntax-literal)" },
  { tag: tags.comment, color: "var(--muted-foreground)", fontStyle: "italic" },
  { tag: [tags.typeName, tags.className], color: "var(--syntax-type)" },
])
const hcl = StreamLanguage.define({
  token(stream) {
    if (stream.match("//") || stream.match("#")) {
      stream.skipToEnd()
      return "comment"
    }
    if (stream.match(/"(?:[^"\\]|\\.)*"/)) return "string"
    if (stream.match(/\b(?:true|false|null)\b/)) return "bool"
    if (stream.match(/\b\d+(?:\.\d+)?\b/)) return "number"
    if (
      stream.match(
        /\b(?:service|backend|principal|principal_setup|collect|issue|effect|async|action|webhook|behavior|scenario|fault|fault_profile|openapi|typescript|listen)\b/
      )
    )
      return "keyword"
    stream.next()
    return null
  },
})
export function Editor({
  value,
  path,
  onChange,
  readOnly = false,
}: {
  value: string
  path: string
  onChange?: (text: string) => void
  readOnly?: boolean
}) {
  const host = useRef<HTMLDivElement>(null),
    view = useRef<EditorView | null>(null),
    change = useRef(onChange)
  useEffect(() => {
    change.current = onChange
  }, [onChange])
  useEffect(() => {
    const language = path.endsWith(".json")
      ? json()
      : path.endsWith(".ts")
        ? javascript({ typescript: true })
        : path.endsWith(".hcl") || path.endsWith("Simfile")
          ? hcl
          : yaml()
    const editor = new EditorView({
      parent: host.current!,
      state: EditorState.create({
        doc: value,
        extensions: [
          lineNumbers(),
          highlightActiveLineGutter(),
          history(),
          keymap.of([...defaultKeymap, ...historyKeymap]),
          syntaxHighlighting(highlighting),
          language,
          EditorView.lineWrapping,
          EditorState.readOnly.of(readOnly),
          EditorView.editable.of(!readOnly),
          EditorView.contentAttributes.of({ "aria-label": path }),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) change.current?.(update.state.doc.toString())
          }),
        ],
      }),
    })
    view.current = editor
    return () => {
      editor.destroy()
      view.current = null
    }
    // File switches create independent editor histories; external updates keep focus.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path, readOnly])
  useEffect(() => {
    const editor = view.current
    if (editor && editor.state.doc.toString() !== value)
      editor.dispatch({
        changes: { from: 0, to: editor.state.doc.length, insert: value },
      })
  }, [value])
  return <div ref={host} className="code-editor" />
}
