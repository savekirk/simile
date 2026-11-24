import {
  Engine,
  type ArtifactResult,
  type BundleFiles,
  type SessionOptions,
  type SessionStatus,
  type Methods,
} from "../worker/client"
import {
  Storage,
  sessionToken,
  type SavedWorkspace,
  type SavedSession,
} from "../storage/idb"
import { starter, workspace, type Project } from "./project"
export interface State {
  project: Project
  mode: "workspace" | "artifact"
  candidate?: ArtifactResult
  active?: ArtifactResult
  compiledRevision: number
  editRevision: number
  compiling: boolean
  diagnostic?: string
  session?: SessionStatus
  sessionOptions: SessionOptions
  savedToken: string
  checkpointState: "unsaved" | "saving" | "saved" | "error"
  projectState: "loading" | "unsaved" | "saving" | "saved" | "error"
  storageError?: string
  bundles: BundleFiles[]
  busy: boolean
  error?: string
}
export function message(error: unknown): string {
  return error && typeof error === "object" && "message" in error
    ? String(error.message)
    : String(error)
}
export class Playground {
  state: State = {
    project: starter(),
    mode: "workspace",
    compiledRevision: -1,
    editRevision: 0,
    compiling: false,
    sessionOptions: { clockMode: "manual" },
    savedToken: "",
    checkpointState: "unsaved",
    projectState: "loading",
    bundles: [],
    busy: false,
  }
  private listeners = new Set<() => void>()
  private compiler = new Engine()
  private runtime = new Engine()
  private storage = new Storage()
  private compileTimer?: ReturnType<typeof setTimeout>
  private saveTimer?: ReturnType<typeof setTimeout>
  private compileRunning = false
  private compileEpoch = 0
  private saveRevision = 0
  private sessionEpoch = 0
  private runtimeLoaded = new Set<string>()
  subscribe = (listener: () => void) => {
    this.listeners.add(listener)
    return () => {
      this.listeners.delete(listener)
    }
  }
  snapshot = () => this.state
  private set(patch: Partial<State>) {
    this.state = { ...this.state, ...patch }
    this.listeners.forEach((listener) => listener())
  }
  async initialize() {
    try {
      const stored = await this.storage.read<SavedWorkspace>("workspace")
      if (this.state.projectState !== "loading") return
      if (stored?.version === 1) {
        let candidate: ArtifactResult | undefined
        let diagnostic: string | undefined
        if (stored.artifact) {
          try {
            candidate = await this.compiler.call("loadArtifact", {
              bytes: stored.artifact.bytes,
            })
          } catch (error) {
            diagnostic = `Saved artifact could not load: ${message(error)}`
          }
        }
        if (this.state.projectState !== "loading") return
        this.set({
          project: stored.project,
          mode: stored.mode,
          candidate,
          diagnostic,
          bundles: stored.bundles,
          projectState: "saved",
        })
      } else this.autosave()
    } catch (error) {
      this.set({ projectState: "error", storageError: message(error) })
    }
    if (this.state.mode === "workspace") this.scheduleCompile()
  }
  edit(project: Project) {
    this.set({
      project,
      mode: "workspace",
      editRevision: this.state.editRevision + 1,
      diagnostic: undefined,
    })
    this.autosave()
    this.scheduleCompile()
  }
  notes(notes: string) {
    this.set({ project: { ...this.state.project, notes } })
    this.autosave()
  }
  private autosave() {
    const version = ++this.saveRevision
    this.set({ projectState: "unsaved" })
    clearTimeout(this.saveTimer)
    this.saveTimer = setTimeout(async () => {
      const captured: SavedWorkspace = {
        version: 1,
        project: this.state.project,
        mode: this.state.mode,
        artifact: this.state.candidate,
        bundles: this.state.bundles,
      }
      this.set({ projectState: "saving" })
      try {
        await this.storage.write("workspace", captured)
        if (version === this.saveRevision)
          this.set({ projectState: "saved", storageError: undefined })
      } catch (error) {
        if (version === this.saveRevision)
          this.set({ projectState: "error", storageError: message(error) })
      }
    }, 350)
  }
  retrySave() {
    this.autosave()
  }
  private scheduleCompile() {
    clearTimeout(this.compileTimer)
    this.compileTimer = setTimeout(() => void this.compile(), 300)
  }
  async compile() {
    clearTimeout(this.compileTimer)
    if (this.compileRunning || this.state.mode !== "workspace") return
    const revision = this.state.editRevision,
      epoch = this.compileEpoch
    this.compileRunning = true
    this.set({ compiling: true })
    try {
      const candidate = await this.compiler.call(
        "compileWorkspace",
        workspace(this.state.project)
      )
      if (
        epoch === this.compileEpoch &&
        revision === this.state.editRevision &&
        this.state.mode === "workspace"
      ) {
        this.set({
          candidate,
          compiledRevision: revision,
          diagnostic: undefined,
        })
        this.autosave()
      }
    } catch (error) {
      if (epoch === this.compileEpoch && revision === this.state.editRevision)
        this.set({ diagnostic: message(error) })
    } finally {
      if (epoch === this.compileEpoch) {
        this.compileRunning = false
        this.set({ compiling: false })
        if (revision !== this.state.editRevision) this.scheduleCompile()
      }
    }
  }
  cancelCompile() {
    clearTimeout(this.compileTimer)
    this.compileEpoch++
    this.compiler.dispose()
    this.compiler = new Engine()
    this.compileRunning = false
    this.set({
      compiling: false,
      diagnostic:
        "Compilation cancelled. Edits and the running session are unchanged.",
    })
  }
  async importArtifact(bytes: Uint8Array) {
    // The candidate is validated before touching the visible workspace or session.
    const candidate = await this.compiler.call("loadArtifact", { bytes })
    this.compileEpoch++
    this.compileRunning = false
    clearTimeout(this.compileTimer)
    this.set({
      mode: "artifact",
      candidate,
      compiling: false,
      diagnostic: undefined,
    })
    this.autosave()
  }
  async candidateCall<K extends "runOperation" | "synthesizeRequest">(
    method: K,
    options: Methods[K]["input"]["options"]
  ) {
    const artifact = this.state.candidate
    if (!artifact) throw new Error("Compile or import an artifact first.")
    await this.loadRuntime(artifact)
    return this.runtime.call(method, {
      revision: artifact.revision,
      options,
    } as Methods[K]["input"])
  }
  private async loadRuntime(artifact: ArtifactResult) {
    if (!this.runtimeLoaded.has(artifact.revision)) {
      await this.runtime.call("loadArtifact", { bytes: artifact.bytes })
      this.runtimeLoaded.add(artifact.revision)
    }
  }
  async start(options: SessionOptions) {
    const active = this.state.candidate
    if (!active) throw new Error("Compile or import an artifact first.")
    await this.loadRuntime(active)
    const session = await this.runtime.call("startSession", {
      revision: active.revision,
      options,
    })
    this.sessionEpoch++
    this.set({
      active,
      session,
      sessionOptions: options,
      savedToken: "",
      checkpointState: "unsaved",
    })
  }
  async call<K extends keyof Methods>(method: K, input: Methods[K]["input"]) {
    const result = await this.runtime.call(method, input)
    if (
      this.state.session &&
      ![
        "inspectState",
        "inspectWork",
        "status",
        "replayBundle",
        "runConformance",
        "prepareWebhook",
      ].includes(method)
    )
      await this.refresh()
    return result
  }
  async refresh() {
    const session = await this.runtime.call("status", {})
    this.set({
      session,
      checkpointState:
        sessionToken(session) === this.state.savedToken ? "saved" : "unsaved",
    })
  }
  async saveCheckpoint() {
    const active = this.state.active
    if (!active) throw new Error("Start a session first.")
    const epoch = this.sessionEpoch
    const options = this.state.sessionOptions
    const captured = await this.runtime.call("exportCheckpoint", {})
    const token = sessionToken(captured.status)
    this.set({ checkpointState: "saving" })
    try {
      await this.storage.write("checkpoint", {
        artifact: active.bytes,
        checkpoint: captured.bytes,
        status: captured.status,
        options,
      } satisfies SavedSession)
      if (epoch === this.sessionEpoch)
        this.set({
          savedToken: token,
          checkpointState:
            sessionToken(this.state.session) === token ? "saved" : "unsaved",
          storageError: undefined,
        })
    } catch (error) {
      if (epoch === this.sessionEpoch)
        this.set({ checkpointState: "error", storageError: message(error) })
      throw error
    }
  }
  async restoreSaved() {
    const saved = await this.storage.read<SavedSession>("checkpoint")
    if (!saved) throw new Error("No completed checkpoint save is available.")
    // Validate both artifacts and checkpoints in a spare Worker. Replace only on success.
    const next = new Engine()
    try {
      const active = await next.call("loadArtifact", { bytes: saved.artifact })
      await next.call("startSession", {
        revision: active.revision,
        options: saved.options,
      })
      const session = await next.call("restoreCheckpoint", {
        bytes: saved.checkpoint,
      })
      this.runtime.dispose()
      this.runtime = next
      this.runtimeLoaded = new Set([active.revision])
      this.sessionEpoch++
      this.set({
        active,
        session,
        sessionOptions: saved.options,
        savedToken: sessionToken(session),
        checkpointState: "saved",
      })
    } catch (error) {
      next.dispose()
      throw error
    }
  }
  async restoreBytes(bytes: Uint8Array) {
    await this.call("restoreCheckpoint", { bytes })
    this.sessionEpoch++
    this.set({ savedToken: "", checkpointState: "unsaved" })
  }
  restartRuntime() {
    this.runtime.dispose()
    this.runtime = new Engine()
    this.runtimeLoaded.clear()
    this.sessionEpoch++
    this.set({
      session: undefined,
      active: undefined,
      savedToken: "",
      checkpointState: "unsaved",
    })
  }
  addBundle(bundle: BundleFiles) {
    if (this.state.bundles.some((item) => item.name === bundle.name))
      throw new Error(
        "A case with that name already exists. Choose a unique name."
      )
    this.set({ bundles: [...this.state.bundles, bundle] })
    this.autosave()
  }
  removeBundle(name: string) {
    this.set({
      bundles: this.state.bundles.filter((bundle) => bundle.name !== name),
    })
    this.autosave()
  }
  async act(action: () => Promise<unknown>) {
    this.set({ busy: true, error: undefined })
    try {
      await action()
    } catch (error) {
      this.set({ error: message(error) })
    } finally {
      this.set({ busy: false })
    }
  }
  clearError() {
    this.set({ error: undefined })
  }
}
