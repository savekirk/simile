import { resolve } from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  worker: { format: "es" },
  base: "./",
  resolve: {
    alias: {
      "../pkg/simile_wasm.js": resolve(
        import.meta.dirname,
        "src/wasm/simile_wasm.js"
      ),
      "@": resolve(import.meta.dirname, "./src"),
    },
  },
})
