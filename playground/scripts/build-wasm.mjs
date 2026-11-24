import { spawnSync } from "node:child_process"
import { resolve } from "node:path"
import { parseArgs } from "node:util"
const root = resolve(import.meta.dirname, "../..")
const { values } = parseArgs({ options: { release: { type: "boolean" } } })
const profile = values.release ? "release" : "debug"
const wasm = `target/wasm32-unknown-unknown/${profile}/simile_wasm.wasm`
function run(command, args) {
  const result = spawnSync(command, args, { cwd: root, stdio: "inherit" })
  if (result.error) throw result.error
  if (result.status !== 0) process.exit(result.status ?? 1)
}
const version = spawnSync("wasm-bindgen", ["--version"], { encoding: "utf8" })
if (version.stdout?.trim() !== "wasm-bindgen 0.2.100") {
  throw new Error(
    "Install the matching binding tool: cargo install wasm-bindgen-cli --version 0.2.100 --locked"
  )
}
run("cargo", [
  "+1.85.0",
  "build",
  "--locked",
  "-p",
  "simile-wasm",
  "--target",
  "wasm32-unknown-unknown",
  ...(values.release ? ["--release"] : []),
])
run("wasm-bindgen", [
  "--target",
  "web",
  "--out-dir",
  "playground/src/wasm",
  wasm,
])
// The shared Worker source and its standalone harness resolve declarations here.
run("wasm-bindgen", [
  "--target",
  "web",
  "--out-dir",
  "crates/simile-wasm/pkg",
  wasm,
])
