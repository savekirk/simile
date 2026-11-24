import { SimileClient } from "../../../crates/simile-wasm/web/client"
import type { Methods } from "../../../crates/simile-wasm/web/contract"
export type {
  ArtifactResult,
  Workspace,
  SessionStatus,
  SessionOptions,
  ReportView,
  BundleFiles,
  HttpExchange,
  WorkItem,
  WorkInspection,
  ProfileSummary,
  StateView,
  Methods,
} from "../../../crates/simile-wasm/web/contract"
export class Engine {
  private worker = new Worker(new URL("./simile.worker.ts", import.meta.url), {
    type: "module",
  })
  private client = new SimileClient(this.worker)
  private pending = new Set<(error: Error) => void>()
  private closed = false
  constructor() {
    this.worker.addEventListener("error", () =>
      this.dispose("Engine failed. Restart it; source edits are preserved.")
    )
    this.worker.addEventListener("messageerror", () =>
      this.dispose("Worker message could not be decoded.")
    )
  }
  async call<K extends keyof Methods>(
    method: K,
    input: Methods[K]["input"]
  ): Promise<Methods[K]["output"]> {
    if (this.closed) throw new Error("Engine stopped. Restart it to continue.")
    return new Promise((resolve, reject) => {
      const abort = (error: Error) => {
        clearTimeout(timer)
        reject(error)
      }
      this.pending.add(abort)
      const timer = setTimeout(() => {
        this.dispose("Engine timed out. Your source edits are preserved.")
      }, 60_000)
      this.client
        .call(method, input)
        .then(resolve, reject)
        .finally(() => {
          clearTimeout(timer)
          this.pending.delete(abort)
        })
    })
  }
  dispose(message = "Engine restarted. Your source edits are preserved.") {
    this.closed = true
    this.worker.terminate()
    for (const reject of this.pending) reject(new Error(message))
    this.pending.clear()
  }
}
