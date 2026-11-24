import { vi, expect, test, beforeEach } from "vitest"
const mock = vi.hoisted(() => ({
  calls: [] as ReturnType<typeof vi.fn>[],
  engines: [] as { isClosed: boolean; call: ReturnType<typeof vi.fn> }[],
  write: vi.fn(),
  read: vi.fn(),
}))
vi.mock("../src/worker/client", () => ({
  Engine: class {
    call = vi.fn()
    isClosed = false
    constructor() {
      mock.calls.push(this.call)
      mock.engines.push(this)
    }
    dispose() {
      this.isClosed = true
    }
  },
}))
vi.mock("../src/storage/idb", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  Storage: class {
    write = mock.write
    read = mock.read
  },
}))
import { Playground } from "../src/model/controller"
import { starter } from "../src/model/project"
import type { ArtifactResult, SessionStatus } from "../src/worker/client"
const artifact = {
  revision: "artifact",
  bytes: new Uint8Array([1]),
} as ArtifactResult
const status: SessionStatus = {
  generation: "1",
  revision: "1",
  artifactDigest: "artifact",
  clockMode: "manual",
  clockMilliseconds: "0",
}
beforeEach(() => {
  mock.calls.length = 0
  mock.engines.length = 0
  mock.write.mockReset()
  mock.read.mockReset()
  vi.useFakeTimers()
})
test("a failed compiler is replaced without touching the active service", async () => {
  const model = new Playground()
  const project = model.state.project
  model.state.active = artifact
  model.state.candidate = artifact
  model.state.session = status
  mock.calls[0].mockImplementation(() => {
    mock.engines[0].isClosed = true
    throw new Error("Engine timed out")
  })
  await model.compile()
  expect(model.state.diagnostic).toContain("timed out")
  expect(model.state.compiling).toBe(false)
  expect(model.state.project).toBe(project)
  expect(model.state.candidate).toBe(artifact)
  const recovered = { ...artifact, revision: "recovered" }
  mock.calls[2].mockResolvedValue(recovered)
  await model.compile()
  expect(model.state.candidate).toBe(recovered)
  expect(model.state.diagnostic).toBeUndefined()
  expect(model.state.active).toBe(artifact)
  expect(model.state.session).toBe(status)
  expect(mock.engines[1].isClosed).toBe(false)
  expect(mock.calls[1]).not.toHaveBeenCalled()
  vi.clearAllTimers()
})

test("artifact imports and queued edits recover after a compiler Worker failure", async () => {
  const model = new Playground()
  model.state.active = artifact
  mock.calls[0].mockImplementation(() => {
    mock.engines[0].isClosed = true
    throw new Error("Worker failed")
  })
  await expect(model.importArtifact(new Uint8Array())).rejects.toThrow(
    "Worker failed"
  )
  mock.calls[2].mockResolvedValue(artifact)
  await model.importArtifact(artifact.bytes)
  expect(model.state.candidate).toBe(artifact)

  model.edit(starter("blank"))
  mock.calls[2].mockImplementation(() => {
    mock.engines[2].isClosed = true
    model.edit({ ...starter("blank"), notes: "Edit during failed compile" })
    throw new Error("Worker failed during compilation")
  })
  await model.compile()
  const next = { ...artifact, revision: "edited" }
  mock.calls[3].mockResolvedValue(next)
  await vi.advanceTimersByTimeAsync(300)
  expect(model.state.candidate).toBe(next)
  expect(model.state.active).toBe(artifact)
  expect(mock.calls[1]).not.toHaveBeenCalled()
  vi.clearAllTimers()
})
test("stale compile results never replace the newer edit revision or active service", async () => {
  const model = new Playground()
  model.state.active = artifact
  let finish!: (value: ArtifactResult) => void
  mock.calls[0].mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve
      })
  )
  const pending = model.compile()
  model.edit(starter("blank"))
  finish(artifact)
  await pending
  expect(model.state.candidate).toBeUndefined()
  expect(model.state.active).toBe(artifact)
  model.cancelCompile()
  vi.clearAllTimers()
})
test("a delayed checkpoint commit cannot mark a newer mutation saved", async () => {
  const model = new Playground()
  model.state.active = artifact
  model.state.session = status
  mock.calls[1].mockResolvedValue({ bytes: new Uint8Array([1]), status })
  let commit!: () => void
  mock.write.mockImplementation(
    () =>
      new Promise<void>((resolve) => {
        commit = resolve
      })
  )
  const saving = model.saveCheckpoint()
  await Promise.resolve()
  await Promise.resolve()
  model.state.session = { ...status, revision: "2" }
  commit()
  await saving
  expect(model.state.checkpointState).toBe("unsaved")
})
test("storage failures keep live runtime state and report an unsaved checkpoint", async () => {
  const model = new Playground()
  model.state.active = artifact
  model.state.session = status
  mock.calls[1].mockResolvedValue({ bytes: new Uint8Array([1]), status })
  mock.write.mockRejectedValue(new Error("Quota exceeded"))
  await expect(model.saveCheckpoint()).rejects.toThrow("Quota")
  expect(model.state.active).toBe(artifact)
  expect(model.state.session).toBe(status)
  expect(model.state.checkpointState).toBe("error")
})
test("failed artifact validation retains the source workspace and live session", async () => {
  const model = new Playground()
  const project = model.state.project
  model.state.active = artifact
  mock.calls[0].mockRejectedValue(new Error("invalid artifact"))
  await expect(model.importArtifact(new Uint8Array())).rejects.toThrow(
    "invalid"
  )
  expect(model.state.project).toBe(project)
  expect(model.state.mode).toBe("workspace")
  expect(model.state.active).toBe(artifact)
})

test("edits made while a saved artifact loads take precedence over restoration", async () => {
  const model = new Playground()
  mock.read.mockResolvedValue({
    version: 1,
    project: starter(),
    mode: "workspace",
    artifact,
    bundles: [],
  })
  let finish!: (value: ArtifactResult) => void
  mock.calls[0].mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve
      })
  )
  const initializing = model.initialize()
  await Promise.resolve()
  const edited = starter("blank")
  model.edit(edited)
  finish(artifact)
  await initializing
  expect(model.state.project).toBe(edited)
  expect(model.state.candidate).toBeUndefined()
  vi.clearAllTimers()
})

test("an incompatible cached artifact cannot discard saved source files or notes", async () => {
  const model = new Playground()
  const project = { ...starter("blank"), notes: "Keep this evidence" }
  mock.read.mockResolvedValue({
    version: 1,
    project,
    mode: "workspace",
    artifact,
    bundles: [],
  })
  mock.calls[0].mockRejectedValue(new Error("unsupported version"))
  await model.initialize()
  expect(model.state.project).toBe(project)
  expect(model.state.diagnostic).toContain("unsupported version")
  expect(model.state.candidate).toBeUndefined()
  vi.clearAllTimers()
})
