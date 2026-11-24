import { unzipSync, zipSync, strToU8 } from "fflate"
import type { BundleFiles, ArtifactResult } from "../worker/client"
import { safePath, type Project } from "./project"
export function download(name: string, bytes: Uint8Array) {
  const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)]))
  const link = document.createElement("a")
  link.href = url
  link.download = name
  link.click()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}
export function unpack(bytes: Uint8Array): Record<string, Uint8Array> {
  let size = 0
  return unzipSync(bytes, {
    filter(file) {
      if (file.name.endsWith("/")) return false
      safePath(file.name)
      size += file.originalSize
      if (size > 64 * 1024 * 1024)
        throw new Error("Archive exceeds 64 MiB expanded.")
      return true
    },
  })
}
export function bundleArchive(bundles: BundleFiles[]): Uint8Array {
  const files: Record<string, Uint8Array> = {}
  for (const bundle of bundles)
    for (const [path, bytes] of Object.entries(bundle.files))
      files[`${safePath(bundle.name)}/${safePath(path)}`] = bytes
  return zipSync(files)
}
export function importBundles(bytes: Uint8Array): BundleFiles[] {
  const files = unpack(bytes)
  const manifests = Object.keys(files).filter(
    (path) => path === "bundle.json" || path.endsWith("/bundle.json")
  )
  if (!manifests.length)
    throw new Error("No native reproduction bundle.json found in the archive.")
  return manifests.map((manifest) => {
    const prefix = manifest.slice(0, -"bundle.json".length)
    return {
      name: prefix.replace(/\/$/, "") || "imported-case",
      files: Object.fromEntries(
        Object.entries(files)
          .filter(([path]) => path.startsWith(prefix))
          .map(([path, data]) => [path.slice(prefix.length), data])
      ),
    }
  })
}
export function handoff(project: Project, artifact: ArtifactResult): string {
  const name = artifact.metadata?.service ?? artifact.service
  const quote = (value: string) => "'" + value.replaceAll("'", "'\\''") + "'"
  const config = project.configPath ?? "Simfile"
  return `# Run from this exported workspace. Requires the matching native Simile version.\n# Compiled snapshot: ${artifact.revision}\nsimile inspect ./service.similec\nsimile audit ./service.similec\n# Registration compiles the full Simfile, including inline setup declarations.\nsimile describe service --config ${quote(config)}\nsimile register --config ${quote(config)}\n${artifact.catalogue.principalSetups.length ? `simile setup ${quote(name)}\nsimile profiles ${quote(name)}\n` : ""}simile start ${quote(name)}\n# Use the credentials issued by native setup for authenticated curl requests.\n# Browser profiles and service checkpoints are separate from native registration.\n`
}
export function runnableProject(project: Project): Project {
  if (project.configPath) return project
  if (!project.contractEntry)
    throw new Error("Choose a contract entry before exporting.")
  if ("Simfile" in project.files)
    throw new Error(
      "Select the existing Simfile or rename it before quick-mode export."
    )
  const format = project.contractEntry.endsWith(".d.ts")
    ? "typescript"
    : "openapi"
  const config = `service "playground-service" {\n  listen {\n    host = "127.0.0.1"\n    port = 4242\n  }\n  ${format} { path = ${JSON.stringify("./" + project.contractEntry)} }\n  modules = ${JSON.stringify(project.modules.map((path) => "./" + path))}\n}\n`
  return {
    ...project,
    configPath: "Simfile",
    files: { ...project.files, Simfile: config },
  }
}
export function workspaceArchive(project: Project, artifact: ArtifactResult) {
  const files: Record<string, Uint8Array> = Object.fromEntries(
    Object.entries(project.files).map(([path, text]) => [
      safePath(path),
      strToU8(text),
    ])
  )
  files["NOTES.md"] = strToU8(project.notes)
  files["HANDOFF.sh"] = strToU8(handoff(project, artifact))
  files["service.similec"] = artifact.bytes
  return zipSync(files)
}
