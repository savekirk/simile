import { test, expect } from "@playwright/test"

for (const failure of ["error", "timeout"] as const) {
  test(`compiler ${failure} recovers without replacing the live service`, async ({
    page,
  }) => {
    await page.addInitScript(() => {
      const control = { failure: "" }
      Object.defineProperty(globalThis, "compilerFailure", {
        set(value: string) {
          control.failure = value
        },
      })
      const NativeWorker = Worker
      globalThis.Worker = class extends NativeWorker {
        postMessage(message: unknown, options?: StructuredSerializeOptions) {
          if (
            control.failure &&
            typeof message === "object" &&
            message !== null &&
            "method" in message &&
            message.method === "compileWorkspace"
          ) {
            const failure = control.failure
            control.failure = ""
            if (failure === "error")
              queueMicrotask(() =>
                this.dispatchEvent(
                  new ErrorEvent("error", {
                    message: "Injected compiler failure",
                  })
                )
              )
            return
          }
          super.postMessage(message, options)
        }
      }
    })
    await page.clock.install()
    await page.goto("/")
    await expect(page.getByText(/Compiled · 4 ops/)).toBeVisible()
    await page.getByRole("tab", { name: "HTTP session", exact: true }).click()
    await page
      .getByRole("button", { name: "Start service", exact: true })
      .click()
    const running = page.getByText(/Running [a-f0-9]{8}/)
    await expect(running).toBeVisible()
    const revision = await running.innerText()
    await page
      .getByText("Session controls & checkpoints", { exact: true })
      .click()
    await page.getByLabel("Advance milliseconds").fill("1234")
    await page.getByRole("button", { name: "Advance", exact: true }).click()
    await expect(
      page.getByText("manual · 1234 ms", { exact: true })
    ).toBeVisible()

    await page.evaluate(
      (failure) => Reflect.set(globalThis, "compilerFailure", failure),
      failure
    )
    await page.getByRole("button", { name: "Compile", exact: true }).click()
    if (failure === "timeout") await page.clock.fastForward(60_001)
    await expect(
      page.getByText("Compilation diagnostic", { exact: true })
    ).toBeVisible()
    await expect(page.locator(".diagnostics")).toContainText(
      failure === "timeout" ? "Engine timed out" : "Engine failed"
    )
    await page.getByRole("button", { name: "Compile", exact: true }).click()
    await expect(
      page.getByText("Compilation diagnostic", { exact: true })
    ).toHaveCount(0)
    await expect(page.getByText(/Compiled · 4 ops/)).toBeVisible()
    await expect(running).toHaveText(revision)
    await page.getByRole("button", { name: "Advance", exact: true }).click()
    await expect(
      page.getByText("manual · 2468 ms", { exact: true })
    ).toBeVisible()
  })
}

test("bundled Petstore compiles all routes and runs baseline and HTTP locally", async ({
  page,
}, testInfo) => {
  const remoteRequests: string[] = []
  page.on("request", (request) => {
    if (new URL(request.url()).hostname === "petstore.swagger.io")
      remoteRequests.push(request.url())
  })
  await page.goto("/")
  await expect(page.getByText(/Compiled · 4 ops/)).toBeVisible()
  await page.getByRole("button", { name: "New workspace", exact: true }).click()
  await page
    .getByRole("menuitem", { name: "Swagger Petstore", exact: true })
    .click()
  await expect(page.getByText(/Compiled · 20 ops/)).toBeVisible()
  await expect(page.getByLabel("Baseline operation")).toHaveValue(
    "POST /store/order"
  )
  await page
    .getByLabel("Load request fixture")
    .selectOption("requests/place-order.json")
  await page.getByRole("button", { name: "Run baseline", exact: true }).click()
  await expect(
    page
      .locator(".execution-content .output")
      .filter({ hasText: "Response body · seed" })
  ).toBeVisible()
  await expect(
    page.getByText("Action could not complete", { exact: true })
  ).toHaveCount(0)
  await page.getByRole("tab", { name: "HTTP session", exact: true }).click()
  await page.getByRole("button", { name: "Start service", exact: true }).click()
  await page
    .getByLabel("HTTP route", { exact: true })
    .selectOption("GET /user/{username}")
  await page.getByLabel("path: username", { exact: true }).fill("doggie")
  await page.getByRole("button", { name: "Send request", exact: true }).click()
  await expect(page.locator(".response-status")).toContainText("200")
  await page.screenshot({
    path: testInfo.outputPath("petstore-http.png"),
    animations: "disabled",
  })
  await page.reload()
  await expect(page.getByText(/Compiled · 20 ops/)).toBeVisible()
  expect(remoteRequests).toEqual([])
})
