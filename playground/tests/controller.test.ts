import { vi, expect, test, beforeEach } from "vitest"
const mock = vi.hoisted(() => ({
  calls: [] as ReturnType<typeof vi.fn>[],
  write: vi.fn(),
  read: vi.fn(),
}))
vi.mock("../src/worker/client", () => ({
  Engine: class {
    call = vi.fn()
    constructor() {
      mock.calls.push(this.call)
    }
    dispose() {}
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
  mock.write.mockReset()
  mock.read.mockReset()
  vi.useFakeTimers()
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
