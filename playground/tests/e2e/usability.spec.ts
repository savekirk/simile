import { readFile } from "node:fs/promises"
import { resolve } from "node:path"
import { test, expect, type Page, type Locator } from "@playwright/test"

async function open(page: Page) {
  await page.goto("/")
  await expect(page.getByText(/Compiled · 4 ops/)).toBeVisible()
}
const source = (page: Page) =>
  page.locator(".source-editor .cm-content[contenteditable=true]")
const request = (page: Page) =>
  page.locator('.cm-content[aria-label="baseline-request.json"]')

async function contrast(locator: Locator, property = "color") {
  return locator.evaluate((element, property) => {
    const canvas = document.createElement("canvas")
    canvas.width = canvas.height = 1
    const context = canvas.getContext("2d")!
    const rgba = (color: string) => {
      context.clearRect(0, 0, 1, 1)
      context.fillStyle = color
      context.fillRect(0, 0, 1, 1)
      return Array.from(context.getImageData(0, 0, 1, 1).data).map(
        (value) => value / 255
      )
    }
    const over = (front: number[], back: number[]) =>
      [0, 1, 2]
        .map((i) => front[i] * front[3] + back[i] * (1 - front[3]))
        .concat(1)
    const ancestors: Element[] = []
    for (let node: Element | null = element; node; node = node.parentElement)
      ancestors.unshift(node)
    let background = [1, 1, 1, 1]
    for (const node of ancestors)
      background = over(
        rgba(getComputedStyle(node).backgroundColor),
        background
      )
    const foreground = over(
      rgba(getComputedStyle(element).getPropertyValue(property)),
      background
    )
    const luminance = (rgb: number[]) =>
      rgb
        .slice(0, 3)
        .map((c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4))
        .reduce((sum, c, i) => sum + c * [0.2126, 0.7152, 0.0722][i], 0)
    const a = luminance(foreground),
      b = luminance(background)
    return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05)
  }, property)
}

for (const theme of ["light", "dark"] as const) {
  test(`${theme}: route drafts, fixtures, formatting, diagnostics, cursor and contrast`, async ({
    page,
  }, testInfo) => {
    const errors: string[] = []
    page.on("pageerror", (error) => errors.push(error.message))
    await page.emulateMedia({ colorScheme: theme })
    await open(page)
    await expect(page.locator("html")).toHaveClass(new RegExp(theme))
    const fixture = page.getByLabel("Load request fixture")
    await fixture.selectOption("requests/create-order.json")
    await expect(fixture).toHaveValue("requests/create-order.json")
    await page.getByLabel("Baseline operation").selectOption("GET /orders/{id}")
    await expect(request(page)).not.toContainText('"body"')
    await page
      .getByRole("button", { name: "Run baseline", exact: true })
      .click()
    await expect(
      page
        .locator(".execution-content .output")
        .filter({ hasText: "Response body · seed" })
    ).toBeVisible()
    await expect(
      page.getByText("Action could not complete", { exact: true })
    ).toHaveCount(0)
    await page.getByLabel("Baseline operation").selectOption("POST /orders")
    await expect(fixture).toHaveValue("requests/create-order.json")
    await expect(request(page)).toContainText('"product"')
    await request(page).fill(
      '{"body":{"product":"browser","quantity":3},"headers":{"Content-Type":"application/json"}}'
    )
    await expect(fixture.locator("option:checked")).toContainText("(modified)")
    await page
      .getByRole("button", {
        name: "Format baseline-request.json",
        exact: true,
      })
      .click()
    await expect(request(page)).toContainText('"quantity": 3')
    await page
      .getByRole("button", { name: "Run baseline", exact: true })
      .click()
    await expect(
      page
        .locator(".execution-content .output")
        .filter({ hasText: "Response body · seed" })
    ).toContainText('"product":"browser"')
    await page.getByLabel("Baseline operation").selectOption("GET /orders/{id}")
    await request(page).fill('{"body":{}}')
    await page
      .getByRole("button", { name: "Run baseline", exact: true })
      .click()
    await expect(
      page.getByRole("tab", { name: "Problems", exact: true })
    ).toHaveAttribute("aria-selected", "true")
    await expect(page.locator(".tool-dock")).toHaveClass(/is-open/)
    await expect(
      page
        .getByRole("alert")
        .filter({ hasText: "does not declare a request body" })
    ).toBeVisible()
    for (const element of await page
      .locator(
        '.diagnostics [data-slot="alert-title"], .diagnostics [data-slot="alert-description"]'
      )
      .all()) {
      expect(
        await contrast(element),
        "error text contrast"
      ).toBeGreaterThanOrEqual(4.5)
    }
    await page.screenshot({
      path: testInfo.outputPath(`${theme}-error.png`),
      animations: "disabled",
    })
    await page.getByRole("button", { name: "Dismiss", exact: true }).click()
    await page.getByRole("button", { name: "Collapse tools" }).click()
    await page
      .getByRole("button", { name: "Synthesize request", exact: true })
      .click()
    await expect(request(page)).not.toContainText('"body"')
    // Compilation failures reveal their file and diagnostic without a navigation step.
    const yaml = await readFile(resolve("src/templates/orders.yaml"), "utf8")
    await source(page).fill("openapi: [broken")
    await expect(
      page.getByText("Compilation diagnostic", { exact: true })
    ).toBeVisible()
    await expect(page.locator(".tool-dock")).toHaveClass(/is-open/)
    await source(page).fill(yaml)
    await expect(
      page.getByText("Compilation diagnostic", { exact: true })
    ).toHaveCount(0)
    await page.getByRole("button", { name: "Collapse tools" }).click()
    await source(page).click()
    const cursor = page.locator(".source-editor .cm-cursor").first()
    await expect(cursor).toBeAttached()
    expect(await contrast(cursor, "border-left-color")).toBeGreaterThanOrEqual(
      3
    )
    await expect(cursor).toHaveCSS("border-left-width", "2px")
    for (const selector of [
      ".source-editor .cm-content",
      ".source-editor .cm-content span",
      ".sidebar-footer",
      ".format-toolbar > span",
      '[data-slot="field-label"]',
      '[data-slot="tabs-trigger"]',
      '[data-slot="native-select"]',
    ]) {
      for (const element of await page.locator(selector).all()) {
        if (await element.isVisible())
          expect(await contrast(element), selector).toBeGreaterThanOrEqual(4.5)
      }
    }
    expect(
      await contrast(page.getByLabel("Baseline operation"), "border-top-color")
    ).toBeGreaterThanOrEqual(3)
    await page.screenshot({
      path: testInfo.outputPath(`${theme}-editor.png`),
      animations: "disabled",
    })
    expect(errors).toEqual([])
  })
}

test("author a new OpenAPI operation, format every source type, and follow contextual help", async ({
  page,
}, testInfo) => {
  await open(page)
  await page.getByRole("button", { name: "Add operation", exact: true }).click()
  await expect(
    page.getByRole("tab", { name: "Guide", exact: true })
  ).toHaveAttribute("aria-selected", "true")
  await expect(
    page.getByRole("link", { name: "Contract authoring", exact: true })
  ).toHaveAttribute(
    "href",
    "https://os.inttegro.dev/simile/define-a-service#2-choose-one-contract-source"
  )
  await expect(
    page.getByRole("link", { name: "Caller profiles", exact: true })
  ).toHaveAttribute(
    "href",
    "https://os.inttegro.dev/simile/cli-reference#manage-caller-profiles"
  )
  await page.getByRole("button", { name: "Open contract", exact: true }).click()
  const yaml = await readFile(resolve("src/templates/orders.yaml"), "utf8")
  await source(page).fill(
    yaml.replace(
      "paths:",
      'paths:\n  /health:\n    get:\n      operationId: health\n      responses:\n        "204": {description: Healthy}'
    )
  )
  await page
    .getByRole("button", { name: "Format orders.yaml", exact: true })
    .click()
  await expect(page.getByText(/Compiled · 5 ops/)).toBeVisible()
  await page.getByRole("button", { name: "Collapse tools" }).click()
  await page.getByLabel("Baseline operation").selectOption("GET /health")
  await page.getByRole("button", { name: "Run baseline", exact: true }).click()
  await expect(
    page
      .locator(".execution-content .output")
      .filter({ hasText: "Response body · seed" })
  ).toContainText("(no JSON body)")
  await page
    .getByRole("navigation", { name: "Open source files" })
    .getByRole("button", { name: "modules/orders.hcl", exact: true })
    .click()
  const hcl = await readFile(resolve("src/templates/orders.hcl"), "utf8")
  await source(page).fill(
    hcl
      .split("\n")
      .map((line) => line.trimStart())
      .join("\n")
  )
  await page
    .getByRole("button", { name: "Format modules/orders.hcl", exact: true })
    .click()
  await expect(source(page)).toContainText("  description =")
  await expect(page.getByText(/Compiled · 5 ops/)).toBeVisible()
  await page
    .getByRole("navigation", { name: "Open source files" })
    .getByRole("button", { name: "Simfile", exact: true })
    .click()
  await page
    .getByRole("button", { name: "Format Simfile", exact: true })
    .click()
  await page.getByRole("button", { name: "New workspace", exact: true }).click()
  await page
    .getByRole("menuitem", { name: "Semantic TypeScript", exact: true })
    .click()
  await expect(page.getByText(/Compiled · 1 ops/)).toBeVisible()
  await page
    .getByRole("navigation", { name: "Open source files" })
    .getByRole("button", { name: "api.d.ts", exact: true })
    .click()
  const ts = await source(page).innerText()
  await source(page).fill(
    ts.replace(
      "export interface OrdersOperations {",
      'export interface OrdersOperations { health: { method: "GET"; path: "/health"; responses: { "204": { description: "Healthy" } } };'
    )
  )
  await page
    .getByRole("button", { name: "Format api.d.ts", exact: true })
    .click()
  await expect(page.getByText(/Compiled · 2 ops/)).toBeVisible()
  await page.getByRole("tab", { name: "HTTP session", exact: true }).click()
  await page.getByRole("button", { name: "Start service", exact: true }).click()
  await page
    .getByLabel("HTTP route", { exact: true })
    .selectOption("GET /health")
  await page.getByRole("button", { name: "Send request", exact: true }).click()
  await expect(page.locator(".response-status")).toContainText("204")
  await page.setViewportSize({ width: 390, height: 844 })
  await expect(
    page.getByRole("region", { name: "Service definition" })
  ).toBeInViewport()
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth
    )
  ).toBe(true)
  await page.screenshot({
    path: testInfo.outputPath("mobile-author-http.png"),
    animations: "disabled",
  })
})
