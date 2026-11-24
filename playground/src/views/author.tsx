import { Editor } from "../components/editor"
import { Choice, PanelHeader, TextField } from "../components/shared"
import { Button } from "../components/ui/button"
import { Badge } from "../components/ui/badge"
import { Alert, AlertTitle, AlertDescription } from "../components/ui/alert"
import { Field, FieldGroup, FieldLabel } from "../components/ui/field"
import { Textarea } from "../components/ui/textarea"
import { Separator } from "../components/ui/separator"
import { PlayIcon, SquareIcon } from "lucide-react"
import type { Playground, State } from "../model/controller"
export function Author({
  model,
  state,
  file,
  onSelectFile,
}: {
  model: Playground
  state: State
  file: string
  onSelectFile: (path: string) => void
}) {
  const project = state.project,
    artifact = state.candidate
  const change = (patch: Partial<typeof project>) =>
    model.edit({ ...project, ...patch })
  return (
    <div className="flex h-full min-h-0 flex-col">
      <PanelHeader
        eyebrow="Workspace"
        title={state.mode === "artifact" ? "Compiled artifact" : project.name}
        description={
          state.mode === "artifact"
            ? "Original sources are unavailable. Inspect and run the compiled service in the other views."
            : "Edit the contract and behavior. Compile a candidate, then start it when you are ready."
        }
      >
        {state.mode === "workspace" && (
          <>
            <Button
              variant="outline"
              onClick={() => model.cancelCompile()}
              disabled={!state.compiling}
            >
              <SquareIcon data-icon="inline-start" />
              Cancel
            </Button>
            <Button
              onClick={() => void model.compile()}
              disabled={state.compiling}
            >
              <PlayIcon data-icon="inline-start" />
              Compile
            </Button>
          </>
        )}
      </PanelHeader>
      {state.diagnostic && (
        <Alert variant="destructive">
          <AlertTitle>Compilation diagnostic</AlertTitle>
          <AlertDescription className="whitespace-pre-wrap">
            {state.diagnostic}
            {Object.keys(project.files)
              .filter((path) => state.diagnostic?.includes(path))
              .map((path) => (
                <Button
                  key={path}
                  variant="link"
                  onClick={() => onSelectFile(path)}
                >
                  Open {path}
                </Button>
              ))}
          </AlertDescription>
        </Alert>
      )}
      <div className="author-grid">
        <div className="editor-panel">
          <div className="editor-title">
            <span>
              {state.mode === "artifact"
                ? "Artifact information"
                : file || "Choose a file"}
            </span>
            <Badge variant="secondary">
              {state.mode === "artifact" ? ".similec" : "Source"}
            </Badge>
          </div>
          {state.mode === "workspace" && file in project.files ? (
            <Editor
              key={file}
              path={file}
              value={project.files[file]}
              onChange={(text) =>
                change({ files: { ...project.files, [file]: text } })
              }
            />
          ) : (
            <div className="p-6 text-muted-foreground">
              {artifact
                ? `${artifact.service} · format ${artifact.formatVersion} · ${artifact.bytes.byteLength.toLocaleString()} bytes`
                : "Create or select a file in the project tree."}
            </div>
          )}
        </div>
        <aside className="inspector">
          <h2>Composition</h2>
          {state.mode === "workspace" && (
            <FieldGroup>
              <Choice
                label="Configuration entry"
                value={project.configPath ?? ""}
                onChange={(value) => change({ configPath: value || undefined })}
                options={[
                  { value: "", label: "Quick mode — choose a contract" },
                  ...Object.keys(project.files)
                    .filter(
                      (path) =>
                        path.endsWith("Simfile") || path.endsWith(".hcl")
                    )
                    .map((path) => ({ value: path, label: path })),
                ]}
              />
              {!project.configPath && (
                <>
                  <Choice
                    label="Contract entry"
                    value={project.contractEntry ?? ""}
                    onChange={(value) =>
                      change({ contractEntry: value || undefined })
                    }
                    options={[
                      { value: "", label: "Select an entry file" },
                      ...Object.keys(project.files)
                        .filter((path) => /\.(yaml|yml|json|d\.ts)$/.test(path))
                        .map((path) => ({ value: path, label: path })),
                    ]}
                  />
                  <TextField
                    label="Behavior modules"
                    hint="Comma-separated virtual paths."
                    value={project.modules.join(", ")}
                    onChange={(value) =>
                      change({
                        modules: value
                          .split(",")
                          .map((path) => path.trim())
                          .filter(Boolean),
                      })
                    }
                  />
                </>
              )}
            </FieldGroup>
          )}
          <Separator />
          <h2>Compiled revision</h2>
          {artifact ? (
            <>
              <dl className="metrics">
                <div>
                  <dt>Operations</dt>
                  <dd>{artifact.operations}</dd>
                </div>
                <div>
                  <dt>Schema nodes</dt>
                  <dd>{artifact.counters.schemaNodes}</dd>
                </div>
                <div>
                  <dt>Effects</dt>
                  <dd>{artifact.counters.effects}</dd>
                </div>
                <div>
                  <dt>Backends</dt>
                  <dd>{artifact.catalogue.backends.length}</dd>
                </div>
                <div>
                  <dt>Principals</dt>
                  <dd>{artifact.catalogue.principals.length}</dd>
                </div>
                <div>
                  <dt>Webhooks</dt>
                  <dd>{artifact.catalogue.webhookContracts.length}</dd>
                </div>
                <div>
                  <dt>Async work</dt>
                  <dd>{artifact.catalogue.asyncOperations.length}</dd>
                </div>
                <div>
                  <dt>Outcomes</dt>
                  <dd>
                    {artifact.catalogue.operations.reduce(
                      (count, operation) => count + operation.outcomes.length,
                      0
                    )}
                  </dd>
                </div>
              </dl>
              <p className="hash">SHA256 {artifact.revision}</p>
            </>
          ) : (
            <p>Compile to inspect the artifact.</p>
          )}
          <Separator />
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="project-notes">Project notes</FieldLabel>
              <Textarea
                id="project-notes"
                rows={7}
                value={project.notes}
                onChange={(event) => model.notes(event.target.value)}
                placeholder="Evidence, assumptions, and accepted fidelity findings…"
              />
            </Field>
          </FieldGroup>
          <p className="text-sm text-muted-foreground">
            Notes are saved with your source files and do not affect
            compilation.
          </p>
        </aside>
      </div>
    </div>
  )
}
