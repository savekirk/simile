import { useEffect, useRef, useState } from "react"
import { Annotation, EditorState, Transaction } from "@codemirror/state"
import {
  EditorView,
  keymap,
  lineNumbers,
  highlightActiveLineGutter,
  drawSelection,
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
import { Button } from "./ui/button"
import { WrapTextIcon } from "lucide-react"
import { fileType, formatText } from "../model/format"
const externalChange = Annotation.define<boolean>()
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
  onError,
}: {
  value: string
  path: string
  onChange?: (text: string) => void
  readOnly?: boolean
  onError?: (error: unknown) => void
}) {
  const [formatting, setFormatting] = useState(false)
  const [formatError, setFormatError] = useState<string>()
  const format = async () => {
    const editor = view.current
    if (!editor || readOnly || formatting) return
    const source = editor.state.doc.toString()
    setFormatting(true)
    setFormatError(undefined)
    try {
      const formatted = await formatText(path, source)
      if (view.current !== editor || editor.state.doc.toString() !== source)
        return
      editor.dispatch({
        changes: { from: 0, to: editor.state.doc.length, insert: formatted },
        userEvent: "input.format",
        selection: {
          anchor: Math.min(
            editor.state.selection.main.anchor,
            formatted.length
          ),
          head: Math.min(editor.state.selection.main.head, formatted.length),
        },
      })
      editor.focus()
    } catch (error) {
      if (view.current === editor) {
        const message = error instanceof Error ? error.message : String(error)
        setFormatError(message)
        onError?.(new Error(`${path}: ${message}`))
      }
    } finally {
      setFormatting(false)
    }
  }
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
          drawSelection(),
          EditorView.theme({
            ".cm-content": { caretColor: "var(--editor-caret)" },
            ".cm-cursor, .cm-dropCursor": {
              borderLeftColor: "var(--editor-caret)",
              borderLeftWidth: "2px",
            },
          }),
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
            if (
              update.docChanged &&
              !update.transactions.some((transaction) =>
                transaction.annotation(externalChange)
              )
            ) {
              setFormatError(undefined)
              change.current?.(update.state.doc.toString())
            }
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
        annotations: [
          externalChange.of(true),
          Transaction.addToHistory.of(false),
        ],
      })
  }, [value])
  return (
    <div className="editor-shell">
      {!readOnly && fileType(path) && (
        <div className="format-toolbar">
          <span>
            {fileType(path) === "hcl"
              ? "HCL · preserves comments and strings"
              : fileType(path)?.toUpperCase()}
          </span>
          <Button
            variant="ghost"
            size="sm"
            aria-label={`Format ${path}`}
            title={
              fileType(path) === "hcl"
                ? "Reindent HCL; preserve inline blocks, comments, and heredocs"
                : "Format document"
            }
            disabled={formatting}
            onClick={() => void format()}
          >
            <WrapTextIcon data-icon="inline-start" />
            {formatting ? "Formatting…" : "Format"}
          </Button>
        </div>
      )}
      {formatError && (
        <p role="alert" className="editor-format-error">
          {formatError}
        </p>
      )}
      <div ref={host} className="code-editor" />
    </div>
  )
}
