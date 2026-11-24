import {
  BookOpenIcon,
  ExternalLinkIcon,
  FilePlusIcon,
  PlayIcon,
} from "lucide-react"
import { Button } from "../components/ui/button"
import { Output } from "../components/shared"
import type { State } from "../model/controller"

export const documentation = {
  author:
    "https://os.inttegro.dev/simile/define-a-service#2-choose-one-contract-source",
  baseline:
    "https://os.inttegro.dev/simile/define-a-service#3-exercise-one-baseline-operation",
  behavior:
    "https://os.inttegro.dev/simile/define-a-service#5-add-the-smallest-useful-module-layer",
  http: "https://os.inttegro.dev/simile/define-a-service#8-verify-the-http-boundary",
  profiles:
    "https://os.inttegro.dev/simile/cli-reference#manage-caller-profiles",
  reproduction:
    "https://os.inttegro.dev/simile/cli-reference#execute-and-reproduce-operations",
}
export function Guide({
  state,
  onOpenFile,
  onRun,
  onHttp,
}: {
  state: State
  onOpenFile: (path: string) => void
  onRun: () => void
  onHttp: () => void
}) {
  const contract =
    state.project.contractEntry ??
    Object.keys(state.project.files).find(
      (path) =>
        /\.(ya?ml|d\.ts|json)$/.test(path) && !path.startsWith("requests/")
    )
  const typescript = contract?.endsWith(".ts")
  return (
    <div className="guide">
      <div>
        <h2>Define, run, inspect</h2>
        <p>
          Edit your contract, compile it, and exercise operations alongside the
          source. Everything runs in this browser.
        </p>
      </div>
      <section>
        <h2>1. Define a new operation</h2>
        <p>
          {typescript
            ? "Add an operation member to your declarations-only TypeScript operations interface. Define its method, path, and responses."
            : "Add a method under a path in your OpenAPI contract. Give it an operationId and response schema; add parameters or a requestBody only when the operation needs them."}{" "}
          HCL adds behavior to contract operations.
        </p>
        <div className="guide-actions">
          <Button
            size="sm"
            variant="outline"
            disabled={state.mode !== "workspace" || !contract}
            onClick={() => contract && onOpenFile(contract)}
          >
            <FilePlusIcon data-icon="inline-start" />
            Open contract
          </Button>
          <Button
            size="sm"
            variant="link"
            render={
              <a href={documentation.author} target="_blank" rel="noreferrer" />
            }
          >
            <ExternalLinkIcon data-icon="inline-start" />
            Contract authoring
          </Button>
        </div>
        <details>
          <summary>Show an operation example</summary>
          <Output
            title={
              typescript
                ? "Add inside your operations interface"
                : "Add under the paths mapping (JSON contracts use the equivalent object)"
            }
          >
            {typescript
              ? 'health: {\n  method: "GET";\n  path: "/health";\n  responses: { "204": { description: "Healthy" } };\n};'
              : '/health:\n  get:\n    operationId: health\n    responses:\n      "204":\n        description: Healthy\n'}
          </Output>
        </details>
        <p>
          Edits compile automatically. New operations appear in the explorer and
          route selector after compilation. The Format button uses the file
          type; HCL formatting reindents existing lines and preserves comments,
          inline blocks, and heredocs.
        </p>
      </section>
      <section>
        <h2>2. Run a baseline</h2>
        <p>
          Select an operation. Each operation has its own request draft,
          synthesized from its contract when first selected. Load a workspace
          fixture or edit the request document: parameters and headers sit
          beside the optional body. A GET that declares no body must omit the
          body field. Baselines start with fresh state; use an HTTP session to
          create a record and read it back.
        </p>
        <div className="guide-actions">
          <Button size="sm" variant="outline" onClick={onRun}>
            <PlayIcon data-icon="inline-start" />
            Open baseline runner
          </Button>
          <Button
            size="sm"
            variant="link"
            render={
              <a
                href={documentation.baseline}
                target="_blank"
                rel="noreferrer"
              />
            }
          >
            Baseline guide
          </Button>
        </div>
      </section>
      <section>
        <h2>3. Exercise stateful HTTP</h2>
        <p>
          Start the service, create a caller profile in Profiles & setup, then
          refresh and enable its credentials under Authentication. Send
          requests, inspect state, and save checkpoints. Restart explicitly to
          apply a newly compiled definition.
        </p>
        <div className="guide-actions">
          <Button size="sm" variant="outline" onClick={onHttp}>
            Open HTTP session
          </Button>
          <Button
            size="sm"
            variant="link"
            render={
              <a href={documentation.http} target="_blank" rel="noreferrer" />
            }
          >
            HTTP verification
          </Button>
          <Button
            size="sm"
            variant="link"
            render={
              <a
                href={documentation.profiles}
                target="_blank"
                rel="noreferrer"
              />
            }
          >
            Caller profiles
          </Button>
        </div>
      </section>
      <section>
        <h2>Go further</h2>
        <div className="guide-actions">
          <Button
            size="sm"
            variant="link"
            render={
              <a
                href={documentation.behavior}
                target="_blank"
                rel="noreferrer"
              />
            }
          >
            <BookOpenIcon data-icon="inline-start" />
            HCL behavior modules
          </Button>
          <Button
            size="sm"
            variant="link"
            render={
              <a
                href={documentation.reproduction}
                target="_blank"
                rel="noreferrer"
              />
            }
          >
            Reproduction & conformance
          </Button>
        </div>
      </section>
    </div>
  )
}
