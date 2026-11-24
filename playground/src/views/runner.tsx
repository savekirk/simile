import { useState } from "react"
import { PlayIcon, WandSparklesIcon, DownloadIcon } from "lucide-react"
import { Button } from "../components/ui/button"
import { FieldGroup } from "../components/ui/field"
import { Editor } from "../components/editor"
import {
  EmptyResult,
  PanelHeader,
  NoArtifact,
  Choice,
  TextField,
  Output,
} from "../components/shared"
import type { Playground, State } from "../model/controller"
import type { ReportView, Methods } from "../worker/client"
import { bundleArchive, download, importBundles } from "../model/archive"
export function Runner({
  model,
  state,
  operation,
  onOperation,
}: {
  model: Playground
  state: State
  operation: string
  onOperation: (value: string) => void
}) {
  const [seed, setSeed] = useState("7"),
    [outcome, setOutcome] = useState(""),
    [scenario, setScenario] = useState("")
  const [request, setRequest] = useState(
    state.project.files["requests/create-order.json"] ?? "{}"
  )
  const [result, setResult] = useState<ReportView>(),
    [resultRevision, setResultRevision] = useState<string>(),
    [name, setName] = useState("create-order")
  const artifact = state.candidate
  if (!artifact) return <NoArtifact />
  const op =
    artifact.routes.find((route) => route.key === operation) ??
    artifact.routes[0]
  const description = artifact.catalogue.operations.find(
    (item) => item.key === op?.key
  )
  return (
    <>
      <FieldGroup className="run-command">
        <Choice
          label="Baseline operation"
          value={op?.key ?? ""}
          onChange={onOperation}
          options={artifact.routes.map((route) => ({
            value: route.key,
            label: `${route.key} · ${route.operationId ?? ""}`,
          }))}
        />
        <Button
          disabled={state.busy || !op}
          onClick={() =>
            void model.act(async () => {
              const response = await model.candidateCall("runOperation", {
                selector: { key: op.key },
                seed: seed || undefined,
                requestDocJson: request,
                outcome: outcome || undefined,
                scenario: scenario || undefined,
                bundleName: name || undefined,
              })
              setResult(response)
              setResultRevision(artifact.revision)
              if (response.seed) setSeed(response.seed)
            })
          }
        >
          <PlayIcon data-icon="inline-start" />
          Run baseline
        </Button>
      </FieldGroup>
      <details className="compact-details">
        <summary>
          Run settings <span>Seed {seed || "auto"}</span>
        </summary>
        <FieldGroup className="runner-controls">
          <TextField
            label="Baseline seed"
            value={seed}
            onChange={setSeed}
            hint="Unsigned 64-bit decimal. Leave empty to generate."
          />
          <Choice
            label="Outcome override"
            value={outcome}
            onChange={setOutcome}
            options={[
              { value: "", label: "Default" },
              ...(description?.outcomes ?? []).map((item) => ({
                value: item.name,
                label: item.name,
              })),
            ]}
          />
          <Choice
            label="Scenario override"
            value={scenario}
            onChange={setScenario}
            options={[
              { value: "", label: "Default" },
              ...artifact.catalogue.scenarios.map((item) => ({
                value: item.name,
                label: item.name,
              })),
            ]}
          />
        </FieldGroup>
      </details>
      <div className="split-panels">
        <section>
          <div className="request-heading">
            <h2>Request document</h2>
            <Button
              size="sm"
              variant="ghost"
              disabled={state.busy || !op}
              onClick={() =>
                void model.act(async () => {
                  const generated = await model.candidateCall(
                    "synthesizeRequest",
                    { selector: { key: op.key }, seed: seed || undefined }
                  )
                  setRequest(generated.requestDocJson)
                  setSeed(generated.seed)
                })
              }
            >
              <WandSparklesIcon data-icon="inline-start" />
              Synthesize request
            </Button>
          </div>
          <FieldGroup>
            <Choice
              label="Load request fixture"
              value=""
              onChange={(path) => {
                if (path) setRequest(state.project.files[path])
              }}
              options={[
                { value: "", label: "Choose a workspace request" },
                ...Object.keys(state.project.files)
                  .filter(
                    (path) =>
                      path.startsWith("requests/") && path.endsWith(".json")
                  )
                  .map((path) => ({ value: path, label: path })),
              ]}
            />
          </FieldGroup>
          <div className="editor-panel mt-3">
            <Editor
              path="baseline-request.json"
              value={request}
              onChange={setRequest}
            />
          </div>
          <details className="compact-details capture-details">
            <summary>Capture reproduction</summary>
            <FieldGroup className="mt-4">
              <TextField
                label="Reproduction case name"
                value={name}
                onChange={setName}
              />
            </FieldGroup>
            <div className="flex flex-wrap gap-2 py-4">
              <Button
                variant="outline"
                disabled={!result?.bundle}
                onClick={() =>
                  void model.act(async () => {
                    if (result?.bundle) model.addBundle(result.bundle)
                  })
                }
              >
                Save reproduction
              </Button>
              <Button
                variant="ghost"
                disabled={!result?.bundle}
                onClick={() =>
                  result?.bundle &&
                  download(
                    `${result.bundle.name}.zip`,
                    bundleArchive([result.bundle])
                  )
                }
              >
                <DownloadIcon data-icon="inline-start" />
                Download bundle
              </Button>
            </div>
          </details>
        </section>
        <section>
          {result ? (
            <>
              {resultRevision !== artifact.revision && (
                <p className="quiet-message">
                  Result from revision {resultRevision?.slice(0, 8)}. Run again
                  to use the current definition.
                </p>
              )}
              <Output title={`Response body · seed ${result.seed}`}>
                {result.bodyJson ?? "(no JSON body)"}
              </Output>
              <details className="compact-details">
                <summary>Execution report</summary>
                <Output title="Validated request, outcome, provenance, and planned webhooks">
                  {result.reportJson}
                </Output>
              </details>
            </>
          ) : (
            <EmptyResult>
              Run a baseline to see its response and execution report.
            </EmptyResult>
          )}
        </section>
      </div>
    </>
  )
}
export function Reproduction({
  model,
  state,
}: {
  model: Playground
  state: State
}) {
  const [report, setReport] = useState<string>(),
    [results, setResults] = useState<Methods["runConformance"]["output"]>()
  return (
    <>
      <PanelHeader
        eyebrow="Portable evidence"
        title="Reproduction & conformance"
        description="Response bundles are native-compatible cases. They are separate from stateful service checkpoints."
      >
        <Button
          variant="outline"
          onClick={() => download("suite.zip", bundleArchive(state.bundles))}
          disabled={!state.bundles.length}
        >
          Export suite
        </Button>
        <Button
          disabled={state.busy || !state.bundles.length}
          onClick={() =>
            void model.act(async () => {
              const result = await model.call("runConformance", state.bundles)
              setResults(result)
              setReport(result.reportJson)
            })
          }
        >
          Run suite
        </Button>
      </PanelHeader>
      <FieldGroup>
        <div>
          <label className="file-picker">
            Import bundle or suite ZIP
            <input
              aria-label="Import bundle or suite ZIP"
              type="file"
              accept=".zip"
              onChange={(event) => {
                const file = event.target.files?.[0]
                if (file)
                  void model.act(async () => {
                    for (const bundle of importBundles(
                      new Uint8Array(await file.arrayBuffer())
                    ))
                      model.addBundle(bundle)
                  })
                event.target.value = ""
              }}
            />
          </label>
        </div>
      </FieldGroup>
      <div className="case-list">
        {state.bundles.map((bundle) => (
          <div key={bundle.name} className="case-row">
            <div>
              <h2>{bundle.name}</h2>
              <p>
                {Object.keys(bundle.files).length} exact files · artifact,
                request, and seed retained
              </p>
            </div>
            <Button
              variant="outline"
              disabled={state.busy}
              onClick={() =>
                void model.act(async () => {
                  const result = await model.call("replayBundle", bundle)
                  setReport(result.reportJson)
                })
              }
            >
              Replay
            </Button>
            <Button
              variant="ghost"
              onClick={() =>
                download(`${bundle.name}.zip`, bundleArchive([bundle]))
              }
            >
              Download
            </Button>
            <Button
              variant="ghost"
              onClick={() => model.removeBundle(bundle.name)}
            >
              Remove
            </Button>
          </div>
        ))}
      </div>
      {!state.bundles.length && (
        <EmptyResult>
          Capture a baseline reproduction or import a bundle to build a suite.
        </EmptyResult>
      )}
      {results && (
        <>
          <h2>
            {results.passed} passed · {results.failed} failed · {results.total}{" "}
            cases
          </h2>
          {results.cases.map((item) => (
            <div className="case-row" key={item.bundle}>
              <strong>{item.passed ? "PASS" : "FAIL"}</strong>
              <span>
                {item.bundle} · {item.operation}
              </span>
              <span>{item.message}</span>
            </div>
          ))}
        </>
      )}
      {report && <Output title="Replay / conformance report">{report}</Output>}
    </>
  )
}
