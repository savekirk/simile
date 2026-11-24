import { Editor } from "../components/editor"
import { Choice, TextField } from "../components/shared"
import { Button } from "../components/ui/button"
import { Badge } from "../components/ui/badge"
import { Alert, AlertTitle, AlertDescription } from "../components/ui/alert"
import { Field, FieldGroup, FieldLabel } from "../components/ui/field"
import { Textarea } from "../components/ui/textarea"
import { Separator } from "../components/ui/separator"
import { HammerIcon, SquareIcon, FileCodeIcon, PlusIcon } from "lucide-react"
import type { Playground, State } from "../model/controller"
type AuthorProps = {
  model: Playground
  state: State
  file: string
  onSelectFile: (path: string) => void
}
export function Author({
  model,
  state,
  file,
  onSelectFile,
  onAddOperation,
}: AuthorProps & { onAddOperation: () => void }) {
  const project = state.project
  return (
    <section className="source-pane" aria-label="Service definition">
      <div className="pane-toolbar">
        <span className="pane-title">
          {state.mode === "artifact" ? "Compiled artifact" : project.name}
        </span>
        {state.mode === "workspace" && (
          <div className="flex items-center gap-1">
            <Button size="sm" variant="ghost" onClick={onAddOperation}>
              <PlusIcon data-icon="inline-start" />
              Add operation
            </Button>
            {state.compiling && (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => model.cancelCompile()}
              >
                <SquareIcon data-icon="inline-start" />
                Cancel
              </Button>
            )}
            <Button
              size="sm"
              variant="outline"
              onClick={() => void model.compile()}
              disabled={state.compiling}
            >
              <HammerIcon data-icon="inline-start" />
              {state.compiling ? "Compiling…" : "Compile"}
            </Button>
          </div>
        )}
      </div>
      {state.mode === "workspace" ? (
        <>
          <nav className="file-tabs" aria-label="Open source files">
            {Object.keys(project.files).map((path) => (
              <button
                key={path}
                title={path}
                aria-current={file === path ? "page" : undefined}
                onClick={() => onSelectFile(path)}
              >
                <FileCodeIcon aria-hidden="true" />
                <span>{path}</span>
              </button>
            ))}
          </nav>
          <div className="source-editor">
            {file in project.files ? (
              <Editor
                key={file}
                path={file}
                onError={(error) => model.reportError(error)}
                value={project.files[file]}
                onChange={(text) =>
                  model.edit({
                    ...project,
                    files: { ...project.files, [file]: text },
                  })
                }
              />
            ) : (
              <p className="p-5 text-sm text-muted-foreground">
                Create or select a file in the explorer.
              </p>
            )}
          </div>
        </>
      ) : (
        <div className="artifact-summary">
          <FileCodeIcon aria-hidden="true" />
          <h1>{state.candidate?.service ?? "Compiled service"}</h1>
          <p>
            This artifact is ready to run. Original sources are not included.
          </p>
          <Badge variant="secondary">
            {state.candidate?.operations ?? 0} operations
          </Badge>
          <p>
            Use Describe below to explore the contract, or import a workspace to
            edit its sources.
          </p>
        </div>
      )}
    </section>
  )
}
export function Diagnostics({
  model,
  state,
  onSelectFile,
}: Omit<AuthorProps, "file">) {
  return (
    <div className="diagnostics">
      {state.error && (
        <Alert variant="destructive">
          <AlertTitle>Action could not complete</AlertTitle>
          <AlertDescription>
            {state.error}
            <Button variant="ghost" onClick={() => model.clearError()}>
              Dismiss
            </Button>
          </AlertDescription>
        </Alert>
      )}
      {state.storageError && (
        <Alert variant="destructive">
          <AlertTitle>Browser storage unavailable</AlertTitle>
          <AlertDescription>
            {state.storageError} Edits and the live session remain usable.
            <Button variant="outline" onClick={() => model.retrySave()}>
              Retry project save
            </Button>
          </AlertDescription>
        </Alert>
      )}
      {state.diagnostic && (
        <Alert variant="destructive">
          <AlertTitle>Compilation diagnostic</AlertTitle>
          <AlertDescription className="whitespace-pre-wrap">
            {state.diagnostic}
            {Object.keys(state.project.files)
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
      {!state.error && !state.storageError && !state.diagnostic && (
        <p className="quiet-message">
          {state.compiling
            ? "Compiling your service…"
            : "No problems to report."}
        </p>
      )}
    </div>
  )
}
export function Composition({
  model,
  state,
}: Pick<AuthorProps, "model" | "state">) {
  const project = state.project,
    artifact = state.candidate
  const change = (patch: Partial<typeof project>) =>
    model.edit({ ...project, ...patch })
  return (
    <div className="inspector">
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
                  (path) => path.endsWith("Simfile") || path.endsWith(".hcl")
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
        Notes are saved with your source files and do not affect compilation.
      </p>
    </div>
  )
}
