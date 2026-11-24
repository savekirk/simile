import { SimileClient } from "../dist/client.js";

const worker = new Worker("/dist/worker.js", { type: "module" });
const client = new SimileClient(worker);
const encode = value => new TextEncoder().encode(value);
const decode = value => new TextDecoder().decode(value);
let assertions = 0;
let generation = "0";
worker.addEventListener("message", event => { generation = event.data.generation; });
function check(value, message) { assertions++; if (!value) throw new Error(message); }
async function rejects(action, code) {
  try { await action(); } catch (error) { check(error.code === code, `expected ${code}, got ${JSON.stringify(error)}`); return; }
  throw new Error(`expected ${code}`);
}
function raw(message) {
  return new Promise(resolve => {
    const receive = event => { if (event.data.id === message.id) { worker.removeEventListener("message", receive); resolve(event.data); } };
    worker.addEventListener("message", receive); worker.postMessage(message);
  });
}
async function main() {
  const files = Object.fromEntries(await Promise.all(["lifecycle.yaml", "lifecycle.hcl"].map(async name => [name, await (await fetch(`/tests/fixtures/${name}`)).text()])));
  const artifact = await client.call("compileWorkspace", { files, contractEntry: "lifecycle.yaml", modules: ["lifecycle.hcl"] });
  check(artifact.bytes instanceof Uint8Array && artifact.bytes.length > 0, "artifact must be transferable bytes");
  check(!(artifact.audit instanceof Map) && Array.isArray(artifact.routes), "metadata maps must be objects");
  check(artifact.routes.find(route => route.key === "POST /nullable").acceptsBody === true, "optional body declaration missing from route metadata");
  check(artifact.routes.find(route => route.key === "POST /orders").acceptsBody === false, "body support must follow the contract, not the HTTP method");
  check(artifact.routes.find(route => route.key === "GET /limits").acceptsBody === false, "body-free route metadata incorrect");
  check(typeof artifact.catalogue.webhooks.reorderWindowMilliseconds === "string", "64-bit metadata must be decimal");
  const revision = artifact.revision;
  const baseline = { revision, options: { selector: { id: "limits" } } };
  for (const seed of ["0", "9007199254740991", "9007199254740992", "18446744073709551615"]) {
    const result = await client.call("runOperation", { ...baseline, options: { ...baseline.options, seed, bundleName: "limits" } });
    check(result.seed === seed, `seed was rounded: ${seed}`);
    check(result.bodyJson.includes("18446744073709551615") && result.bodyJson.includes("-9223372036854775808"), "response JSON integers were rounded");
    check(result.bundle.files["service.similec"] instanceof Uint8Array, "bundle file must be typed bytes");
    check(decode(result.bundle.files["bundle.json"]).includes(seed), "bundle seed was changed by JavaScript");
    check((await client.call("replayBundle", result.bundle)).matched, "bundle did not replay");
    check((await client.call("runConformance", [result.bundle])).passed === 1, "conformance failed");
  }
  await rejects(() => client.call("runOperation", { ...baseline, options: { ...baseline.options, seed: "18446744073709551616" } }), "invalid_decimal");
  await rejects(() => client.call("runOperation", { ...baseline, options: { ...baseline.options, seed: 9007199254740992 } }), "invalid_input");
  const nullable = requestDocJson => client.call("runOperation", { revision, options: { selector: { id: "nullable" }, seed: "1", requestDocJson, bundleName: "nullable" } });
  const absent = await nullable('{}');
  const nil = await nullable('{"headers":{"content-type":"application/json"},"body":null}');
  check(!absent.requestDocJson.includes('"body"') && nil.requestDocJson.includes('"body":null'), "absent and null bodies were collapsed");
  check((await client.call("replayBundle", nil.bundle)).matched, "null body was lost in bundle replay");
  check((await nullable(nil.requestDocJson)).requestDocJson === nil.requestDocJson, "returned request document cannot be reused");
  const nested = await nullable('{"headers":{"content-type":"application/json"},"body":{"nested":{"high":18446744073709551615,"low":-9223372036854775808}}}');
  check(nested.requestDocJson.includes("18446744073709551615") && nested.requestDocJson.includes("-9223372036854775808"), "request payload was rounded");
  for (const startMilliseconds of ["-9223372036854775808", "9223372036854775807", "0"]) {
    const status = await client.call("startSession", { revision, options: { startMilliseconds } });
    check(status.clockMilliseconds === startMilliseconds, "signed clock boundary changed");
  }
  await rejects(() => client.call("startSession", { revision, options: { startMilliseconds: "9223372036854775808" } }), "invalid_decimal");
  check((await client.call("status", {})).clockMilliseconds === "0", "failed session construction replaced the active session");
  const call = responseSeed => client.call("handleRequest", { method: "POST", uri: "/orders", headers: [["idempotency-key", encode("same")]], responseSeed });
  const first = await call("1");
  check(first.body instanceof Uint8Array && first.status === 201, "HTTP body or status incorrect");
  check(decode((await call("2")).body) === decode(first.body), "live replay cache did not retain the first outcome");
  const work = await client.call("stepWork", { limit: 10 });
  check(work.some(item => item.kind === "async") && work.some(item => item.kind === "webhook"), "deferred work was not exposed");
  check(!work.some(item => item.name === "order.ready"), "dependent webhook escaped readiness checks");
  const hook = work.find(item => item.kind === "webhook");
  const prepared = await client.call("prepareWebhook", { handle: hook.handle, timestamp: "18446744073709551615" });
  check(prepared.body instanceof Uint8Array && prepared.uri === "https://example.test/hook", "webhook bytes or target changed");
  const checkpoint = await client.call("exportCheckpoint", {});
  check(checkpoint.bytes instanceof Uint8Array, "checkpoint must be canonical bytes");
  const originalCheckpoint = decode(checkpoint.bytes);
  for (const [pattern, replacement] of [
    [/"version":1/, '"version":2'],
    [/"artifactFormat":\d+/, '"artifactFormat":65535'],
    [/"stackProtocol":\d+/, '"stackProtocol":65535'],
    [/"service":"[^"]+"/, '"service":"another-service"'],
    [/"nextEvent":\d+/, '"nextEvent":0'],
    [/"resources":\[[^\]]+\]/, '"resources":[]'],
  ]) {
    const incompatible = originalCheckpoint.replace(pattern, replacement);
    check(incompatible !== originalCheckpoint, "incompatible checkpoint fixture did not change");
    await rejects(() => client.call("restoreCheckpoint", { bytes: encode(incompatible) }), "checkpoint_incompatible");
    check(decode((await client.call("exportCheckpoint", {})).bytes) === originalCheckpoint, "incompatible checkpoint mutated active state");
  }
  await rejects(() => client.call("loadArtifact", { bytes: encode("broken") }), "artifact_incompatible");
  const unsupported = artifact.bytes.slice();
  new DataView(unsupported.buffer).setUint16(8, 65535);
  const versionError = await raw({ id: "format-test", generation, method: "loadArtifact", input: { bytes: unsupported } });
  check(!versionError.ok && versionError.error.code === "artifact_incompatible" && versionError.error.message.includes("65535") && versionError.error.message.includes(String(artifact.formatVersion)) && versionError.error.message.includes("Recompile"), "format diagnostic needs found/supported versions and a remedy");
  const corrupted = artifact.bytes.slice();
  corrupted[corrupted.length - 1] ^= 1;
  await rejects(() => client.call("loadArtifact", { bytes: corrupted }), "artifact_incompatible");
  await rejects(() => client.call("restoreCheckpoint", { bytes: encode("broken") }), "checkpoint_incompatible");
  check(decode((await client.call("exportCheckpoint", {})).bytes) === originalCheckpoint, "invalid import mutated active state");
  await rejects(() => client.call("compileWorkspace", { files: { "bad.yaml": "not openapi" }, contractEntry: "bad.yaml" }), "compile_error");
  check((await client.call("status", {})).artifactDigest === revision, "failed compilation replaced active revision");
  const beforeRestore = generation;
  await client.call("restoreCheckpoint", { bytes: checkpoint.bytes });
  await rejects(() => client.call("acknowledgeWork", { handle: hook.handle }), "stale_handle");
  const stale = await raw({ id: "stale-test", generation: beforeRestore, method: "status", input: {} });
  check(!stale.ok && stale.error.code === "stale_generation", "old generation was accepted");
  check((await client.call("stepWork", { limit: 10 })).length === 0, "restore discarded queue visibility timeouts");
  await client.call("advanceClock", { milliseconds: "30000" });
  const recovered = await client.call("stepWork", { limit: 10 });
  check(recovered.length > 0 && recovered.every(item => !work.some(old => old.handle === item.handle)), "restored work reused old handles");
  await client.call("executeWork", { handle: recovered.find(item => item.kind === "async").handle });
  await client.call("acknowledgeWork", { handle: recovered.find(item => item.kind === "webhook").handle });
  await client.call("advanceClock", { milliseconds: "250" });
  const dependent = (await client.call("stepWork", { limit: 10 })).find(item => item.name === "order.ready");
  check(dependent?.kind === "webhook", "acknowledgement did not release dependent work");
  check((await client.call("retryWork", { handle: dependent.handle, delay: "10" })).retrying, "work retry was not scheduled");
  await rejects(() => client.call("acknowledgeWork", { handle: dependent.handle }), "stale_handle");
  check((await client.call("stepWork", { limit: 10 })).length === 0, "retry ignored its delay");
  await client.call("advanceClock", { milliseconds: "10" });
  const retried = (await client.call("stepWork", { limit: 10 }))[0];
  check(retried.name === "order.ready" && retried.attempt > dependent.attempt, "retry lost work identity or attempt count");
  await client.call("acknowledgeWork", { handle: retried.handle });
  check(decode((await call("2")).body) !== decode(first.body), "restore preserved the process-local replay cache");
  await client.call("resetState", {});
  check((await client.call("stepWork", { limit: 10 })).length === 0, "reset retained outbox work");
  check((await client.call("listProfiles", {})).length === 0, "reset retained dynamic profiles");
  check((await client.call("status", {})).clockMilliseconds === "0", "reset did not restore initial time");
  check(decode((await call("1")).body) === decode(first.body), "reset did not construct a fresh runtime");
  check(artifact.bytes.byteLength > 0, "caller artifact buffer was detached");
  const loaded = await client.call("loadArtifact", { bytes: artifact.bytes });
  check(loaded.revision === revision, "reloading transferred artifact bytes changed revision");
  const smithyBytes = new Uint8Array(await (await fetch("/tests/fixtures/orders.smithy.similec")).arrayBuffer());
  const smithy = await client.call("loadArtifact", { bytes: smithyBytes });
  const smithyRun = await client.call("runOperation", { revision: smithy.revision, options: { selector: { id: "createOrder" }, seed: "0" } });
  check(smithyRun.reportJson.includes('"status":201'), "CLI-produced Smithy artifact did not execute");
  await rejects(() => client.call("compileWorkspace", { files: { "orders.smithy": "" }, contractEntry: "orders.smithy" }), "unsupported_source");
  check((await client.call("status", {})).artifactDigest === revision, "candidate imports changed active session");
  const invalid = await raw({ id: "invalid-test", generation, method: "stepWork", input: { limit: -1 } });
  check(!invalid.ok && invalid.error.code === "invalid_message", "negative limit crossed the Worker boundary");
  const badBody = await raw({ id: "body-test", generation, method: "handleRequest", input: { method: "POST", uri: "/nullable", body: null } });
  check(!badBody.ok && badBody.error.code === "invalid_message", "JSON null must be body bytes, not a missing body");
  const httpArtifact = await client.call("compileWorkspace", { files: { "http.yaml": await (await fetch("/shared/http-parity.yaml")).text() }, contractEntry: "http.yaml" });
  const parameters = httpArtifact.routes.find(route => route.operationId === "getItem").parameters;
  check(parameters.some(parameter => parameter.name === "tag" && parameter.location === "query" && parameter.required && parameter.explode), "route parameter metadata is incomplete");
  const synthesized = await client.call("synthesizeRequest", { revision: httpArtifact.revision, options: { selector: { id: "createItem" }, seed: "18446744073709551615" } });
  check(synthesized.body instanceof Uint8Array && synthesized.seed === "18446744073709551615" && synthesized.contentType === "application/json", "request synthesis changed bytes or seed");
  const synthesizedRun = await client.call("runOperation", { revision: httpArtifact.revision, options: { selector: { id: "createItem" }, requestDocJson: synthesized.requestDocJson, seed: "0" } });
  check(synthesizedRun.reportJson.includes('"status":201'), "synthesized request cannot be executed");
  await client.call("startSession", { revision: httpArtifact.revision });
  const repeated = await client.call("handleRequest", { method: "GET", uri: "/items/7?tag=one&tag=two", headers: [["x-role", encode("reader")], ["x-role", encode("writer")]], responseSeed: "0" });
  check(repeated.status === 200 && repeated.headers.every(pair => pair[1] instanceof Uint8Array), "repeated headers or byte header values were lost");
  const single = await client.call("handleRequest", { method: "GET", uri: "/items/7?tag=one&tag=two", headers: [["x-role", encode("reader")]], responseSeed: "0" });
  check(single.status === 400, "repeated header fixture did not validate both values");
  await rejects(() => client.call("restoreCheckpoint", { bytes: checkpoint.bytes }), "checkpoint_incompatible");
  check((await client.call("status", {})).artifactDigest === httpArtifact.revision, "incompatible checkpoint replaced active artifact");
  const setupFiles = {
    Simfile: await (await fetch("/shared/service-described.hcl")).text(),
    "run-orders.yaml": await (await fetch("/shared/run-orders.yaml")).text(),
  };
  const setupArtifact = await client.call("compileWorkspace", { files: setupFiles });
  await client.call("startSession", { revision: setupArtifact.revision, options: { startMilliseconds: "-9223372036854775808" } });
  const profile = await client.call("setupProfile", { principal: "merchant", name: "fixture", valuesJson: '{"product":"fixture","quantity":18446744073709551615}', select: true, seed: "0" });
  check(profile.createdAt === "-9223372036854775808" && profile.profileJson.includes("18446744073709551615"), "profile timestamp or collected JSON lost precision");
  check((await client.call("listProfiles", {}))[0].createdAt === "-9223372036854775808", "profile metadata exposed a numeric timestamp");
  const state = await client.call("inspectState", { includeValues: true, afterSequence: "0" });
  check(state.events.length > 0 && state.events.every(event => typeof event.sequence === "string" && typeof event.commandId === "string" && typeof event.at === "string"), "journal identifiers must be decimal strings");
  check(state.reportJson.includes("18446744073709551615"), "state payload JSON lost precision");
  const profileCheckpoint = await client.call("exportCheckpoint", {});
  await client.call("resetState", {});
  check((await client.call("listProfiles", {})).length === 0, "reset retained a created profile");
  await client.call("restoreCheckpoint", { bytes: profileCheckpoint.bytes });
  check((await client.call("listProfiles", {}))[0].name === "fixture", "checkpoint lost a dynamic profile");
  await client.call("startSession", { revision, options: { faultProfile: "seeded" } });
  const faultSequence = async () => {
    const kinds = [];
    for (let count = 0; count < 12; count++) {
      const exchange = await client.call("handleRequest", { method: "GET", uri: "/limits", responseSeed: "0" });
      kinds.push(exchange.transport.kind);
      if (exchange.transport.kind === "delay") check(exchange.transport.milliseconds === "10", "fault delay must be a decimal string");
    }
    return kinds.join(",");
  };
  const faults = await faultSequence();
  check(faults.includes("delay") && faults.includes("respond"), "fault fixture must exercise RNG progress");
  const faultCheckpoint = await client.call("exportCheckpoint", {});
  await client.call("restoreCheckpoint", { bytes: faultCheckpoint.bytes });
  check(await faultSequence() === faults, "restore did not restart fault RNG progress");
  await client.call("resetState", {});
  check(await faultSequence() === faults, "reset did not restart fault RNG progress");
  for (const receiveAgain of [false, true]) {
    await client.call("startSession", { revision });
    await call("1");
    const expired = await client.call("stepWork", { limit: 10 });
    await client.call("advanceClock", { milliseconds: "30000" });
    let current = receiveAgain ? await client.call("stepWork", { limit: 10 }) : [];
    const before = decode((await client.call("exportCheckpoint", {})).bytes);
    for (const item of expired) {
      const actions = [
        ["acknowledgeWork", { handle: item.handle }],
        ["retryWork", { handle: item.handle, delay: "0" }],
        item.kind === "async"
          ? ["executeWork", { handle: item.handle }]
          : ["prepareWebhook", { handle: item.handle, timestamp: "0" }],
      ];
      for (const [method, input] of actions) {
        await rejects(() => client.call(method, input), "stale_handle");
        check(decode((await client.call("exportCheckpoint", {})).bytes) === before, `${method} mutated state through an expired handle`);
      }
    }
    if (!receiveAgain) current = await client.call("stepWork", { limit: 10 });
    const async = current.find(item => item.kind === "async");
    check(expired.some(item => item.id === async.id && item.handle !== async.handle), "renewed work must retain its identity with a new handle");
    await client.call("executeWork", { handle: async.handle });
    const completed = decode((await client.call("exportCheckpoint", {})).bytes);
    await rejects(() => client.call("executeWork", { handle: async.handle }), "stale_handle");
    check(decode((await client.call("exportCheckpoint", {})).bytes) === completed, "completed work executed again");
    const hook = current.find(item => item.kind === "webhook");
    check((await client.call("retryWork", { handle: hook.handle, delay: "0" })).retrying, "renewed webhook could not retry");
    check((await client.call("stepWork", { limit: 10 })).filter(item => item.name === hook.name).length === 1, "retry duplicated the original webhook");
  }
  await client.call("startSession", { revision, options: { clockMode: "realtime" } });
  check((await client.call("status", {})).clockMode === "realtime", "Worker realtime clock did not initialize");
  check(Math.abs(Number((await client.call("status", {})).clockMilliseconds) - Date.now()) < 10_000, "realtime session did not start at browser wall time");
  await rejects(() => client.call("advanceClock", { milliseconds: "1" }), "operation_failed");
  return assertions;
}
main().then(count => { globalThis.simileTestResult = { ok: true, assertions: count }; })
  .catch(error => { globalThis.simileTestResult = { ok: false, error: error.stack ?? JSON.stringify(error) }; })
  .finally(() => worker.terminate());
