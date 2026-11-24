import { test, expect } from "@playwright/test"
import { readFile } from "node:fs/promises"
import { resolve } from "node:path"
async function compiled(page: import("@playwright/test").Page) {
  await page.goto("/")
  await expect(page.getByText(/Compiled · 4 ops/)).toBeVisible()
}
test("author, audit, provision, create/read, work, checkpoint, and reproduce", async ({
  page,
}) => {
  const errors: string[] = []
  page.on("pageerror", (error) => errors.push(error.message))
  await compiled(page)
  await page
    .getByLabel("Project notes")
    .fill("Reviewed Orders API assumptions.")
  await expect(page.getByRole("status")).toContainText("Project saved")
  await page.getByRole("tab", { name: "Audit", exact: true }).click()
  await expect(
    page.getByRole("heading", { name: "Audit the contract" })
  ).toBeVisible()
  await page.getByRole("tab", { name: "Workbench", exact: true }).click()
  await page.getByRole("button", { name: "Start service", exact: true }).click()
  await expect(page.getByText(/Running [a-f0-9]{8}/)).toBeVisible()
  await page.getByRole("tab", { name: "Profiles & setup" }).click()
  await page.getByLabel("name", { exact: true }).fill("Browser integration")
  await page
    .getByRole("button", { name: "Create profile", exact: true })
    .click()
  await expect(
    page.getByText("Credentials issued. Use the profile toggle")
  ).toBeVisible()
  await expect(page.getByText(/sk_simile_/)).toHaveCount(0)
  await page.getByRole("tab", { name: "HTTP request", exact: true }).click()
  await page.getByRole("button", { name: "Refresh profiles" }).click()
  await page.getByRole("switch", { name: "Use profile credentials" }).check()
  await page.getByRole("button", { name: "Send request", exact: true }).click()
  await expect(page.locator(".response-status")).toContainText("201")
  const body = await page
    .locator(".output")
    .filter({ hasText: "Response body · exact" })
    .locator("pre")
    .innerText()
  const id = JSON.parse(body).id
  await page
    .getByLabel("HTTP route", { exact: true })
    .selectOption("GET /orders/{id}")
  await page.getByLabel("path: id", { exact: true }).fill(id)
  await page.getByRole("button", { name: "Send request", exact: true }).click()
  await expect(page.locator(".response-status")).toContainText("200")
  await expect(
    page
      .locator(".output")
      .filter({ hasText: "Response body · exact" })
      .locator("pre")
  ).toContainText(id)
  await page.getByRole("tab", { name: "Work & webhooks" }).click()
  await page.getByRole("button", { name: "Inspect outbox" }).click()
  await expect(page.locator(".work-card")).toHaveCount(2)
  await page.getByRole("button", { name: "Advance", exact: true }).click()
  await page.getByRole("button", { name: "Step work", exact: true }).click()
  await page.getByRole("button", { name: "Execute async", exact: true }).click()
  await page
    .getByRole("button", { name: "Simulate success", exact: true })
    .click()
  await expect(page.locator(".work-card")).toHaveCount(0)
  await page
    .getByRole("button", { name: "Save checkpoint", exact: true })
    .click()
  await expect(
    page.getByText("Checkpoint saved", { exact: true })
  ).toBeVisible()
  await page.reload()
  await expect(page.getByLabel("Project notes")).toHaveValue(
    "Reviewed Orders API assumptions."
  )
  await page.getByRole("tab", { name: "Workbench", exact: true }).click()
  await page.getByRole("button", { name: "Restore saved", exact: true }).click()
  await expect(page.getByText(/Running [a-f0-9]{8}/)).toBeVisible()
  await page.getByRole("tab", { name: "Stack & journal" }).click()
  await page.getByLabel("Backend resource").selectOption("db.primary")
  await page.getByRole("button", { name: "Inspect snapshot" }).click()
  await expect(page.locator(".output pre")).toContainText(id)
  await page.getByRole("tab", { name: "Baseline", exact: true }).click()
  await page.getByRole("button", { name: "Run baseline", exact: true }).click()
  await expect(
    page.getByRole("button", { name: "Save reproduction" })
  ).toBeEnabled()
  await page.getByRole("button", { name: "Save reproduction" }).click()
  await page.getByRole("tab", { name: "Reproduction", exact: true }).click()
  await page.getByRole("button", { name: "Replay", exact: true }).click()
  await expect(page.locator(".output pre")).toContainText('"matched":true')
  await page.getByRole("button", { name: "Run suite", exact: true }).click()
  await expect(
    page.getByRole("heading", { name: "1 passed · 0 failed · 1 cases" })
  ).toBeVisible()
  expect(errors).toEqual([])
})
test("invalid artifacts preserve the session and CLI Smithy artifacts run", async ({
  page,
}) => {
  await compiled(page)
  await page.getByRole("tab", { name: "Workbench", exact: true }).click()
  await page.getByRole("button", { name: "Start service", exact: true }).click()
  const running = page.getByText(/Running [a-f0-9]{8}/)
  await expect(running).toBeVisible()
  const text = await running.innerText()
  await page.getByLabel("Import files", { exact: true }).setInputFiles({
    name: "bad.similec",
    mimeType: "application/octet-stream",
    buffer: Buffer.from("invalid"),
  })
  await expect(
    page.getByRole("alert").filter({ hasText: "Action could not complete" })
  ).toBeVisible()
  await expect(running).toHaveText(text)
  await page.getByRole("button", { name: "Dismiss" }).click()
  await page
    .getByLabel("Import files", { exact: true })
    .setInputFiles(
      resolve("../crates/simile-wasm/tests/fixtures/orders.smithy.similec")
    )
  await expect(
    page.getByText("COMPILED ARTIFACT", { exact: true })
  ).toBeVisible()
  await page.getByRole("tab", { name: "Baseline", exact: true }).click()
  await page.getByRole("button", { name: "Synthesize request" }).click()
  await page.getByRole("button", { name: "Run baseline", exact: true }).click()
  await expect(
    page.getByRole("button", { name: "Save reproduction" })
  ).toBeEnabled()
})
test("exports a runnable workspace and exact reproduction files", async ({
  page,
}, testInfo) => {
  await compiled(page)
  await page.getByRole("button", { name: "Export", exact: true }).click()
  const event = page.waitForEvent("download")
  await page.getByRole("menuitem", { name: "Runnable workspace ZIP" }).click()
  const downloaded = await event
  const path = testInfo.outputPath("workspace.zip")
  await downloaded.saveAs(path)
  expect((await readFile(path)).byteLength).toBeGreaterThan(500)
  await page.getByRole("tab", { name: "Baseline", exact: true }).click()
  await page.getByRole("button", { name: "Run baseline", exact: true }).click()
  const bundleEvent = page.waitForEvent("download")
  await page.getByRole("button", { name: "Download bundle" }).click()
  const bundle = await bundleEvent
  const bundlePath = testInfo.outputPath("bundle.zip")
  await bundle.saveAs(bundlePath)
  await page.getByRole("tab", { name: "Reproduction", exact: true }).click()
  await page.getByLabel("Import bundle or suite ZIP").setInputFiles(bundlePath)
  await page.getByRole("button", { name: "Replay", exact: true }).click()
  await expect(page.locator(".output pre")).toContainText('"matched":true')
})

test("compilation keeps the active revision until an explicit restart", async ({
  page,
}) => {
  await compiled(page)
  await page.getByRole("tab", { name: "Workbench", exact: true }).click()
  await page.getByRole("button", { name: "Start service", exact: true }).click()
  const running = await page.getByText(/Running [a-f0-9]{8}/).innerText()
  await page.getByRole("tab", { name: "Author", exact: true }).click()
  const yaml = await readFile(resolve("src/templates/orders.yaml"), "utf8")
  await page
    .locator(".cm-content[contenteditable=true]")
    .fill(yaml.replace("title: Orders", "title: Orders revision two"))
  await expect(page.getByText("Source edits pending")).toHaveCount(0)
  await page.getByRole("tab", { name: "Workbench", exact: true }).click()
  await expect(page.getByText("A different revision is running")).toBeVisible()
  await expect(page.getByText(/Running [a-f0-9]{8}/)).toHaveText(running)
  await page.getByRole("button", { name: "Restart with controls" }).click()
  await expect(page.getByText(/Running [a-f0-9]{8}/)).not.toHaveText(running)
  await expect(page.getByText("A different revision is running")).toHaveCount(0)
})

test("TypeScript starter compiles and mobile navigation stays in bounds", async ({
  page,
}) => {
  await compiled(page)
  await page.getByRole("button", { name: "New workspace" }).click()
  await page.getByRole("menuitem", { name: "Semantic TypeScript" }).click()
  await expect(page.getByText(/Compiled · 1 ops/)).toBeVisible()
  await page.setViewportSize({ width: 390, height: 844 })
  await expect(
    page.getByRole("button", { name: "Project", exact: true })
  ).toBeVisible()
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth
    )
  ).toBe(true)
  await page.getByRole("tab", { name: "Workbench", exact: true }).click()
  await page.getByRole("button", { name: "Start service", exact: true }).click()
  await expect(page.getByText(/Running [a-f0-9]{8}/)).toBeVisible()
  for (const name of [
    "Author",
    "Describe",
    "Audit",
    "Baseline",
    "Reproduction",
    "Workbench",
    "Profiles & setup",
    "Stack & journal",
    "Work & webhooks",
    "HTTP request",
  ]) {
    await page.getByRole("tab", { name, exact: true }).click()
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= window.innerWidth
      ),
      name
    ).toBe(true)
  }
  await page.getByRole("button", { name: "Project", exact: true }).click()
  await expect(
    page.getByRole("heading", { name: "Project explorer" })
  ).toBeVisible()
})

test("storage failures retain edits and the live session, then retry", async ({
  page,
}) => {
  await page.addInitScript(() => {
    const open = indexedDB.open.bind(indexedDB)
    indexedDB.open = (...args) => {
      if (!document.documentElement.hasAttribute("data-test-storage-enabled"))
        throw new DOMException(
          "Storage temporarily unavailable",
          "SecurityError"
        )
      return open(...args)
    }
  })
  await compiled(page)
  await page
    .getByLabel("Project notes")
    .fill("Unsaved evidence survives failed storage.")
  await expect(
    page.getByText("Browser storage unavailable", { exact: true })
  ).toBeVisible()
  await page.getByRole("tab", { name: "Workbench", exact: true }).click()
  await page.getByRole("button", { name: "Start service", exact: true }).click()
  const running = page.getByText(/Running [a-f0-9]{8}/)
  await expect(running).toBeVisible()
  const revision = await running.innerText()
  await page.evaluate(() =>
    document.documentElement.setAttribute("data-test-storage-enabled", "")
  )
  await page
    .getByRole("button", { name: "Retry project save", exact: true })
    .click()
  await expect(page.getByRole("status")).toContainText("Project saved")
  await expect(running).toHaveText(revision)
  await page.getByRole("tab", { name: "Author", exact: true }).click()
  await expect(page.getByLabel("Project notes")).toHaveValue(
    "Unsaved evidence survives failed storage."
  )
})
