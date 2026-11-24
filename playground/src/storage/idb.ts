import type {
  ArtifactResult,
  BundleFiles,
  SessionOptions,
  SessionStatus,
} from "../worker/client"
import type { Project } from "../model/project"
export interface SavedSession {
  artifact: Uint8Array
  checkpoint: Uint8Array
  status: SessionStatus
  options: SessionOptions
}
export interface SavedWorkspace {
  version: 1
  project: Project
  mode: "workspace" | "artifact"
  artifact?: ArtifactResult
  bundles: BundleFiles[]
}
export class Storage {
  private database?: Promise<IDBDatabase>
  private open() {
    if (this.database) return this.database
    const opening = new Promise<IDBDatabase>((resolve, reject) => {
      let failed = false
      const fail = (error: unknown) => {
        failed = true
        reject(error)
      }
      const request = indexedDB.open("simile-playground", 1)
      request.onupgradeneeded = () =>
        request.result.createObjectStore("records")
      request.onerror = () => fail(request.error)
      request.onblocked = () =>
        fail(
          new Error("Close other playground tabs to update browser storage.")
        )
      request.onsuccess = () => {
        if (failed) {
          request.result.close()
          return
        }
        request.result.onversionchange = () => {
          request.result.close()
          this.database = undefined
        }
        resolve(request.result)
      }
    }).catch((error: unknown) => {
      this.database = undefined
      throw error
    })
    this.database = opening
    return opening
  }
  async read<T>(key: string): Promise<T | undefined> {
    const db = await this.open()
    return new Promise((resolve, reject) => {
      const tx = db.transaction("records", "readonly")
      const request = tx.objectStore("records").get(key)
      tx.oncomplete = () => resolve(request.result as T | undefined)
      tx.onerror = tx.onabort = () =>
        reject(tx.error ?? new Error("Storage read failed."))
    })
  }
  async write(key: string, value: unknown): Promise<void> {
    const db = await this.open()
    return new Promise((resolve, reject) => {
      const tx = db.transaction("records", "readwrite")
      tx.objectStore("records").put(value, key)
      // A successful request is not proof of a committed transaction.
      tx.oncomplete = () => resolve()
      tx.onerror = tx.onabort = () =>
        reject(tx.error ?? new Error("Storage save failed."))
    })
  }
}
export function sessionToken(status?: SessionStatus): string {
  return status
    ? `${status.artifactDigest}:${status.generation}:${status.revision}`
    : ""
}
