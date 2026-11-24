import orders from "../templates/orders.yaml?raw"
import behavior from "../templates/orders.hcl?raw"
import setup from "../templates/setup.hcl?raw"
import petstore from "../templates/petstore/openapi.json?raw"
import petstoreGuide from "../templates/petstore/README.md?raw"
import petstoreLicense from "../templates/petstore/LICENSE.txt?raw"
import type { Workspace } from "../../../crates/simile-wasm/web/contract"
export interface Project {
  name: string
  files: Record<string, string>
  notes: string
  configPath?: string
  contractEntry?: string
  modules: string[]
}
export function starter(kind = "orders"): Project {
  if (kind === "petstore")
    return {
      name: "Swagger Petstore",
      notes:
        "Bundled from https://petstore.swagger.io/v2/swagger.json and converted to OpenAPI 3.0.3. See README.md for provenance and examples. Responses run locally; no requests reach the public Petstore.",
      modules: [],
      contractEntry: "petstore.openapi.json",
      files: {
        "petstore.openapi.json": petstore,
        "requests/place-order.json":
          '{\n  "headers": { "Content-Type": "application/json" },\n  "body": { "id": 10, "petId": 7, "quantity": 1, "shipDate": "2026-11-24T00:00:00Z", "status": "placed", "complete": false }\n}\n',
        "README.md": petstoreGuide,
        "LICENSE.txt": petstoreLicense,
      },
    }
  if (kind === "typescript")
    return {
      name: "TypeScript service",
      notes: "A multi-file declarations-only contract.",
      configPath: "Simfile",
      modules: [],
      files: {
        Simfile:
          'service "typed-orders" {\n  listen {\n    host = "127.0.0.1"\n    port = 4242\n  }\n  typescript { path = "./api.d.ts" }\n  modules = ["./modules/orders.hcl"]\n}\n',
        "api.d.ts":
          'import type { Order } from "./models";\nexport type ServiceName = "Typed Orders";\nexport interface OrdersOperations {\n  getOrder: { method: "GET"; path: "/orders"; responses: { "200": { content: { "application/json": Order } } } };\n}\n',
        "models.d.ts":
          "export interface Order { readonly id: string; readonly product: string; }\n",
        "modules/orders.hcl": 'backend "db" "primary" {}\n',
      },
    }
  if (kind === "blank")
    return {
      name: "Untitled service",
      notes: "",
      modules: [],
      contractEntry: "api.yaml",
      files: {
        "api.yaml":
          'openapi: 3.1.0\ninfo: { title: My service, version: 1.0.0 }\npaths:\n  /health:\n    get:\n      operationId: health\n      responses:\n        "204": { description: Healthy }\n',
      },
    }
  return {
    name: "Orders workspace",
    notes:
      "Evidence: docs/define-a-service.md. This example uses synthetic credentials and a simulated webhook recipient. Review audit findings before local handoff.",
    configPath: "Simfile",
    modules: [],
    files: {
      Simfile:
        'service "orders" {\n  description = "A local emulator for the public Orders API."\n  listen {\n    host = "127.0.0.1"\n    port = 4242\n  }\n  openapi { path = "./orders.yaml" }\n  modules = ["./modules/orders.hcl"]\n}\n' +
        setup,
      "orders.yaml": orders,
      "modules/orders.hcl": behavior,
      "requests/create-order.json":
        '{\n  "headers": { "Content-Type": "application/json" },\n  "body": { "product": "licensed_beat", "quantity": 2 }\n}\n',
    },
  }
}
export function workspace(project: Project): Workspace {
  return {
    files: { ...project.files },
    ...(project.configPath
      ? { configPath: project.configPath }
      : { contractEntry: project.contractEntry, modules: project.modules }),
  }
}
export function safePath(path: string): string {
  const normalized = path.replaceAll("\\", "/")
  if (
    !normalized ||
    normalized.startsWith("/") ||
    normalized.includes(":") ||
    normalized.split("/").some((part) => !part || part === "." || part === "..")
  )
    throw new Error("Use a relative path without empty segments or ..")
  return normalized
}
export function renameFile(
  project: Project,
  from: string,
  to: string
): Project {
  to = safePath(to)
  if (to !== from && to in project.files)
    throw new Error("A file already exists at that path.")
  const files = { ...project.files, [to]: project.files[from] }
  if (to !== from) delete files[from]
  return {
    ...project,
    files,
    configPath: project.configPath === from ? to : project.configPath,
    contractEntry: project.contractEntry === from ? to : project.contractEntry,
    modules: project.modules.map((path) => (path === from ? to : path)),
  }
}
