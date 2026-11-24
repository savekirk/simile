// cf deploy dist publishes the prebuilt app as static assets, without server code.
export default {
  accountId: "d127ce3a8b8fd77bc9e73713c90a7991",
  worker: {
    name: "similator",
    compatibilityDate: "2026-11-24",
    workersDev: false,
    previewUrls: false,
    domains: ["similator.dela.dev"],
    assets: { notFoundHandling: "single-page-application" },
  },
}
