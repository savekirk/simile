import { expect, test } from "vitest"
import { safePath, renameFile, starter } from "../src/model/project"
import {
  bundleArchive,
  importBundles,
  runnableProject,
  unpack,
} from "../src/model/archive"
import { setupValues } from "../src/model/setup"
import { zipSync } from "fflate"
import originalPetstore from "../src/templates/petstore/swagger.json?raw"

test("the bundled Petstore conversion retains the upstream operations and schemas", () => {
  const project = starter("petstore")
  const source = JSON.parse(originalPetstore)
  const contract = JSON.parse(project.files[project.contractEntry!])
  const operations = (paths: Record<string, Record<string, unknown>>) =>
    Object.entries(paths)
      .flatMap(([path, methods]) =>
        Object.keys(methods)
          .filter((method) =>
            /^(get|post|put|delete|patch|head|options)$/.test(method)
          )
          .map((method) => `${method} ${path}`)
      )
      .sort()
  expect(contract.openapi).toBe("3.0.3")
  expect(operations(contract.paths)).toEqual(operations(source.paths))
  expect(operations(contract.paths)).toHaveLength(20)
  expect(Object.keys(contract.components.schemas).sort()).toEqual(
    Object.keys(source.definitions).sort()
  )
  expect(Object.keys(contract.components.securitySchemes).sort()).toEqual(
    Object.keys(source.securityDefinitions).sort()
  )
  expect(contract.servers).toContainEqual({
    url: "https://petstore.swagger.io/v2",
  })
  expect(project.files["LICENSE.txt"]).toContain("Apache License")
})
test("virtual paths and archive entries cannot escape the project", () => {
  for (const path of ["../secret", "/root/file", "a/../b", "a//b", "C:/file"])
    expect(() => safePath(path)).toThrow()
  expect(safePath("models/order.d.ts")).toBe("models/order.d.ts")
  expect(() => unpack(zipSync({ "../file": new Uint8Array([1]) }))).toThrow()
  expect(() => renameFile(starter(), "orders.yaml", "Simfile")).toThrow()
})
test("quick exports include an explicit runnable composition", () => {
  const result = runnableProject(starter("blank"))
  expect(result.configPath).toBe("Simfile")
  expect(result.files.Simfile).toContain("./api.yaml")
  expect(runnableProject(result)).toEqual(result)
})
test("native bundle archives preserve bytes and ordered named cases", () => {
  const bundles = [
    {
      name: "create",
      files: {
        "bundle.json": new TextEncoder().encode(
          '{"seed":18446744073709551615}'
        ),
        "artifact.similec": new Uint8Array([0, 1, 255]),
      },
    },
  ]
  expect(importBundles(bundleArchive(bundles))).toEqual(bundles)
})
test("setup controls preserve integer and nested JSON values", () => {
  const fields = ["integer", "json", "boolean"].map((valueType) => ({
    name: valueType,
    valueType,
    required: true,
    requestPointer: "/" + valueType,
  }))
  expect(
    setupValues(fields, {
      integer: "18446744073709551615",
      json: '{"n":9223372036854775807}',
      boolean: "false",
    })
  ).toBe(
    '{"integer":18446744073709551615,"json":{"n":9223372036854775807},"boolean":false}'
  )
  expect(() => setupValues(fields, {})).toThrow("required")
})
