import "fake-indexeddb/auto"
import { expect, test, vi } from "vitest"
import { Storage, sessionToken } from "../src/storage/idb"
test("only committed records are read, including exact bytes", async () => {
  const storage = new Storage()
  const bytes = new Uint8Array([0, 255, 1, 129])
  await storage.write("test", { bytes, revision: "18446744073709551615" })
  const saved = await new Storage().read<{
    bytes: Uint8Array
    revision: string
  }>("test")
  expect(saved).toEqual({ bytes, revision: "18446744073709551615" })
})
test("generation and artifact are part of checkpoint identity", () => {
  const original = {
    artifactDigest: "a",
    generation: "1",
    revision: "1",
    clockMode: "manual" as const,
    clockMilliseconds: "0",
  }
  expect(sessionToken(original)).not.toEqual(
    sessionToken({ ...original, generation: "2" })
  )
  expect(sessionToken(original)).not.toEqual(
    sessionToken({ ...original, artifactDigest: "b" })
  )
})

test("storage can retry after an unavailable database", async () => {
  const storage = new Storage()
  const open = vi.spyOn(indexedDB, "open").mockImplementationOnce(() => {
    throw new DOMException("Storage unavailable", "SecurityError")
  })
  await expect(storage.write("retry", "first")).rejects.toThrow(
    "Storage unavailable"
  )
  await storage.write("retry", "second")
  expect(await storage.read("retry")).toBe("second")
  open.mockRestore()
})
