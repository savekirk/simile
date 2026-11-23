import init, { ArtifactHandle, BrowserSession, compileWorkspace, loadArtifact, replayBundle, runConformance } from "../pkg/simile_wasm.js";
import { validateRequest, type ArtifactResult, type ArtifactView, type Diagnostic, type Request, type Response } from "./contract.js";

const initialized = init();
const artifacts = new Map<string, ArtifactHandle>();
let session: BrowserSession | undefined;
let generation = 0n;
let queue = Promise.resolve();

function diagnostic(error: unknown): Diagnostic {
  if (error && typeof error === "object" && "code" in error && "message" in error && typeof error.code === "string" && typeof error.message === "string") return error as Diagnostic;
  return { code: "worker_error", message: error instanceof Error ? error.message : String(error) };
}
function active(): BrowserSession {
  if (!session) throw { code: "no_session", message: "start a session first" };
  return session;
}
function artifact(revision: string): ArtifactHandle {
  const handle = artifacts.get(revision);
  if (!handle) throw { code: "unknown_revision", message: "compile or load this artifact first" };
  return handle;
}
function candidate(handle: ArtifactHandle): ArtifactResult {
  const view = handle.view() as ArtifactView;
  const bytes = handle.bytes(); // A copy; transferring it cannot detach the retained artifact.
  artifacts.get(view.revision)?.free();
  artifacts.set(view.revision, handle);
  return { ...view, bytes };
}

function dispatch(message: Request): unknown {
  switch (message.method) {
    case "compileWorkspace": return candidate(compileWorkspace(message.input));
    case "loadArtifact": return candidate(loadArtifact(message.input.bytes));
    case "synthesizeRequest": return artifact(message.input.revision).synthesizeRequest(message.input.options);
    case "runOperation": return artifact(message.input.revision).runOperation(message.input.options);
    case "replayBundle": return replayBundle(message.input);
    case "runConformance": return runConformance(message.input);
    case "startSession": {
      const next = new BrowserSession(artifact(message.input.revision), message.input.options ?? {});
      let status: unknown;
      try { status = next.status(); } catch (error) { next.free(); throw error; }
      session?.free(); session = next; generation++;
      return status;
    }
    case "status": return active().status();
    case "handleRequest": return active().handleRequest(message.input);
    case "setupProfile": return active().setupProfile(message.input);
    case "listProfiles": return active().listProfiles();
    case "inspectState": return active().inspectState(message.input);
    case "advanceClock": return active().advanceClock(message.input.milliseconds);
    case "resetState": { const result = active().resetState(); generation++; return result; }
    case "exportCheckpoint": return active().exportCheckpoint();
    case "restoreCheckpoint": { const result = active().restoreCheckpoint(message.input.bytes); generation++; return result; }
    case "stepWork": return active().stepWork(message.input.limit);
    case "prepareWebhook": return active().prepareWebhook(message.input.handle, message.input.timestamp);
    case "executeWork": return active().executeWork(message.input.handle);
    case "acknowledgeWork": return active().acknowledgeWork(message.input.handle);
    case "retryWork": return active().retryWork(message.input.handle, message.input.delay);
  }
}

/** Transfer only response-owned buffers. Retained Rust artifacts are never detached. */
function transfers(value: unknown, found = new Set<ArrayBuffer>()): ArrayBuffer[] {
  if (value instanceof Uint8Array && value.buffer instanceof ArrayBuffer) found.add(value.buffer);
  else if (Array.isArray(value)) for (const child of value) transfers(child, found);
  else if (value && typeof value === "object") for (const child of Object.values(value)) transfers(child, found);
  return [...found];
}

self.onmessage = (event: MessageEvent<unknown>) => {
  queue = queue.then(async () => {
    let id = "invalid";
    let response: Response;
    try {
      await initialized;
      validateRequest(event.data);
      const message = event.data;
      id = message.id;
      if (BigInt(message.generation) !== generation) throw { code: "stale_generation", message: "session changed before this request ran" };
      response = { id, generation: generation.toString(), ok: true, value: dispatch(message) };
      response.generation = generation.toString();
    } catch (error) {
      if (event.data && typeof event.data === "object" && "id" in event.data && typeof event.data.id === "string") id = event.data.id;
      response = { id, generation: generation.toString(), ok: false, error: diagnostic(error) };
    }
    self.postMessage(response, { transfer: transfers(response) });
  });
};
