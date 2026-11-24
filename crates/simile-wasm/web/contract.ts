/** Decimal strings are range-checked by Rust. Payload JSON is opaque UTF-8 text. */
export type Decimal = string;
export type Header = [name: string, value: Uint8Array];
export type Selector = { key: string } | { id: string };
export interface Workspace {
  files: Record<string, string>;
  configPath?: string;
  contractEntry?: string;
  contractFormat?: "auto" | "openapi" | "typescript" | "smithy";
  modules?: string[];
  heuristicPolicy?: "standard" | "off" | "conservative" | "aggressive";
}
export interface Diagnostic { code: string; message: string; file?: string }
export interface Parameter { name: string; location: "path" | "query" | "header" | "cookie"; required: boolean; style: string; explode: boolean; allowReserved: boolean }
export interface Route { key: string; method: string; path: string; operationId: string | null; acceptsBody: boolean; parameters: Parameter[]; effects: number; webhooks: number }
export interface NamedDescription { name: string; description?: string }
export interface OperationDescription {
  key: string; operationId?: string; description?: string; behaviorSource: string; secured: boolean;
  outcomes: (NamedDescription & { status: number })[];
}
export interface SetupDescription {
  principal: string; description?: string; operationId: string;
  collects: (NamedDescription & { required: boolean; valueType: string; requestPointer: string; binding?: string })[];
  issues: (NamedDescription & { source: string; credential?: string; sensitive: boolean; binding?: string })[];
}
export interface Catalogue {
  service: string; description?: string; serviceVersion: string; openapiVersion: string; operationCount: number;
  operations: OperationDescription[]; principalSetups: SetupDescription[];
  scenarios: (NamedDescription & { selections: number; faults: number })[];
  faults: (NamedDescription & { phase: string; action: string })[];
  faultProfiles: (NamedDescription & { probabilityPpm: number; faults: string[]; operations: string[] })[];
  backends: { reference: string; description?: string }[];
  asyncOperations: (NamedDescription & { backends: string[] })[];
  protocolErrors: { cause: string; description?: string; statuses: number[] }[];
  webhooks: { ordering: string; outOfOrderProbabilityPpm: number; reorderWindowMilliseconds: Decimal; concurrency: number; signed: boolean; signatureHeader?: string };
  webhookContracts: (NamedDescription & { method: string; origin: string; operationId?: string; target?: string; contentTypes: string[] })[];
  authoringObjects: { kind: string; parent?: string; name: string; description: string }[];
  principals: (NamedDescription & { securitySchemes: string[]; grants: string[] })[];
  profiles: (NamedDescription & { principal: string; status: string; securitySchemes: string[] })[];
}
export interface AuditReport {
  service: string; serviceVersion: string; artifactSha256: string; operations: number;
  providerAuthoredOutcomes: number; providerAuthoredEffects: number; conventionInferredEffects: number;
  securedOperations: number; principals: number; webhooks: number; signedWebhooks: boolean;
  errors: number; warnings: number;
  findings: { severity: "info" | "warning" | "error"; code: string; subject: string; message: string; remediation: string }[];
}
export interface ArtifactView {
  revision: string; formatVersion: number; service: string; serviceVersion: string;
  operations: number; routes: Route[];
  counters: { schemaNodes: number; effects: number; webhooks: number; backends: number; principals: number; outcomes: number; asyncOperations: number };
  catalogue: Catalogue;
  audit: AuditReport;
  metadata: { service: string; source: string; listen?: string; reportingEndpoint?: string } | null;
}
export interface ArtifactResult extends ArtifactView { bytes: Uint8Array }
export interface BundleFiles { name: string; files: Record<string, Uint8Array> }
export interface ReportView {
  reportJson: string; seed?: Decimal; requestDocJson?: string; bodyJson?: string;
  bundle?: BundleFiles; matched?: boolean;
}
export interface RunOptions {
  selector: Selector; seed?: Decimal; outcome?: string; scenario?: string;
  requestDocJson?: string; contentType?: string; bundleName?: string;
}
export interface SessionOptions {
  clockMode?: "manual" | "realtime"; startMilliseconds?: Decimal;
  scenario?: string; faultProfile?: string; responseSeed?: Decimal; bodyLimit?: number;
}
export interface SessionStatus {
  generation: Decimal; revision: Decimal; clockMode: "manual" | "realtime";
  clockMilliseconds: Decimal; artifactDigest: string;
}
export interface HttpRequest {
  method: string; uri: string; headers?: Header[]; body?: Uint8Array; responseSeed?: Decimal;
  profile?: string;
}
export interface HttpExchange {
  status: number; headers: Header[]; body: Uint8Array; seed: Decimal;
  operation?: string; profile?: string;
  transport: { kind: "respond" } | { kind: "delay"; milliseconds: Decimal } | { kind: "disconnect_after_commit" };
  fault?: { name: string; phase: string; source: { kind: "scenario" | "profile"; name: string } };
  incident?: Diagnostic;
}
export interface SetupOptions { principal?: string; name?: string; valuesJson: string; select?: boolean; seed?: Decimal }
export interface SetupResult { seed: Decimal; created: boolean; selected: boolean; profileJson: string; name: string; id: string; createdAt: Decimal; updatedAt: Decimal }
export interface ProfileSummary { id: string; name: string; principal: string; status: string; createdAt: Decimal; updatedAt: Decimal; issued: { name: string; sensitive: boolean }[]; credentialSchemes: string[]; current: boolean; serviceRevision: string; webhookTargetConfigured: boolean; webhookSigningConfigured: boolean }
export interface StateQuery { resource?: string; causationId?: string; correlationId?: string; afterSequence?: Decimal; limit?: number; includeValues?: boolean }
export type JournalOutcome = { status: "succeeded" } | { status: "failed"; code: "conflict" | "invalid_command" | "invalid_value" | "kind_mismatch" | "not_found" | "undeclared_resource" | "unavailable" };
export interface StateView { reportJson: string; resources: string[]; totalEvents: number; events: { sequence: Decimal; commandId: Decimal; at: Decimal; resource: string; action: string; outcome: JournalOutcome }[] }
export interface WorkItem { handle: string; id: string; name: string; kind: "async" | "webhook"; attempt: number; workJson: string }
export interface PreparedWebhook { method: string; uri: string; headers: Header[]; body: Uint8Array }
export interface WorkInspection { id: string; name: string; kind: "async" | "webhook"; deadLetter: boolean; leased: boolean; availableAt: Decimal; readiness: "ready" | "waiting" | "blocked"; workJson: string }
export interface Methods {
  inspectWork: { input: Record<string, never>; output: WorkInspection[] };
  redeliverDeadLetter: { input: Record<string, never>; output: { redelivered: boolean; status: SessionStatus } };
  compileWorkspace: { input: Workspace; output: ArtifactResult };
  loadArtifact: { input: { bytes: Uint8Array }; output: ArtifactResult };
  synthesizeRequest: { input: { revision: string; options: { selector: Selector; seed?: Decimal; contentType?: string } }; output: { seed: Decimal; requestDocJson: string; contentType?: string; body?: Uint8Array } };
  runOperation: { input: { revision: string; options: RunOptions }; output: ReportView };
  replayBundle: { input: BundleFiles; output: ReportView };
  runConformance: { input: BundleFiles[]; output: { reportJson: string; total: number; passed: number; failed: number; cases: { bundle: string; operation: string | null; passed: boolean; message: string }[] } };
  startSession: { input: { revision: string; options?: SessionOptions }; output: SessionStatus };
  status: { input: Record<string, never>; output: SessionStatus };
  handleRequest: { input: HttpRequest; output: HttpExchange };
  setupProfile: { input: SetupOptions; output: SetupResult };
  listProfiles: { input: Record<string, never>; output: ProfileSummary[] };
  inspectState: { input: StateQuery; output: StateView };
  advanceClock: { input: { milliseconds: Decimal }; output: { clockMilliseconds: Decimal; reportJson: string } };
  resetState: { input: Record<string, never>; output: SessionStatus };
  exportCheckpoint: { input: Record<string, never>; output: { bytes: Uint8Array; status: SessionStatus } };
  restoreCheckpoint: { input: { bytes: Uint8Array }; output: SessionStatus };
  stepWork: { input: { limit: number }; output: WorkItem[] };
  prepareWebhook: { input: { handle: string; timestamp: Decimal }; output: PreparedWebhook };
  executeWork: { input: { handle: string }; output: SessionStatus };
  acknowledgeWork: { input: { handle: string }; output: SessionStatus };
  retryWork: { input: { handle: string; delay: Decimal }; output: { retrying: boolean; status: SessionStatus } };
}
export type Request = { [K in keyof Methods]: { id: string; generation: Decimal; method: K; input: Methods[K]["input"] } }[keyof Methods];
export type Response = { id: string; generation: Decimal } & ({ ok: true; value: unknown } | { ok: false; error: Diagnostic });

/** Validate the untrusted Worker envelope; Rust validates operation DTOs and ranges. */
export function validateRequest(value: unknown): asserts value is Request {
  const fail = (message: string): never => { throw { code: "invalid_message", message } satisfies Diagnostic; };
  if (!value || typeof value !== "object" || Array.isArray(value)) fail("expected a message object");
  const message = value as Record<string, unknown>;
  if (typeof message.id !== "string" || !message.id || message.id.length > 128) fail("invalid request id");
  if (typeof message.generation !== "string" || !/^\d+$/.test(message.generation)) fail("generation must be a decimal string");
  if (typeof message.method !== "string" || !methods.has(message.method)) fail("unknown method");
  if (!message.input || typeof message.input !== "object") fail("expected an input object");
  for (const key of Object.keys(message)) if (!["id", "generation", "method", "input"].includes(key)) fail(`unknown message field ${key}`);
  const input = message.input as Record<string, unknown>;
  for (const key of ["seed", "responseSeed", "afterSequence", "milliseconds", "timestamp", "delay"]) {
    if (key in input && typeof input[key] !== "string") fail(`${key} must be a decimal string`);
  }
  for (const key of ["limit", "bodyLimit"]) if (key in input && (!Number.isSafeInteger(input[key]) || (input[key] as number) < 0 || (input[key] as number) > 0xffff_ffff)) fail(`${key} must be an unsigned 32-bit integer`);
  for (const key of ["bytes", "body"]) if (key in input && !(input[key] instanceof Uint8Array)) fail(`${key} must be Uint8Array`);
  if (message.method === "stepWork" && !("limit" in input)) fail("stepWork requires a limit");
  if (message.method === "loadArtifact" || message.method === "restoreCheckpoint") {
    if (!(input.bytes instanceof Uint8Array)) fail("bytes must be Uint8Array");
  }
  if ("headers" in input && (!Array.isArray(input.headers) || !input.headers.every(pair => Array.isArray(pair) && pair.length === 2 && typeof pair[0] === "string" && pair[1] instanceof Uint8Array))) fail("headers must be ordered name/byte pairs");
  const requiredStrings: Partial<Record<keyof Methods, string[]>> = {
    advanceClock: ["milliseconds"], prepareWebhook: ["handle", "timestamp"],
    executeWork: ["handle"], acknowledgeWork: ["handle"], retryWork: ["handle", "delay"],
    startSession: ["revision"], synthesizeRequest: ["revision"], runOperation: ["revision"],
  };
  for (const key of requiredStrings[message.method as keyof Methods] ?? []) if (typeof input[key] !== "string" || !input[key]) fail(`${key} must be a nonempty string`);
  if (message.method === "compileWorkspace") {
    if (!input.files || typeof input.files !== "object" || Array.isArray(input.files) || input.files instanceof Map) fail("files must be an object map");
  }
  const bundle = (value: unknown) => {
    if (!value || typeof value !== "object" || !("name" in value) || typeof value.name !== "string" || !("files" in value)) fail("invalid bundle");
    const files = (value as BundleFiles).files;
    if (!files || typeof files !== "object" || Array.isArray(files) || files instanceof Map || !Object.values(files).every(bytes => bytes instanceof Uint8Array)) fail("bundle files must be an object map of Uint8Array");
  };
  if (message.method === "replayBundle") bundle(message.input);
  if (message.method === "runConformance") {
    if (!Array.isArray(message.input)) fail("conformance requires an array of named bundles");
    (message.input as unknown[]).forEach(bundle);
  }
}
const methods = new Set<string>(["inspectWork", "redeliverDeadLetter","compileWorkspace", "loadArtifact", "synthesizeRequest", "runOperation", "replayBundle", "runConformance", "startSession", "status", "handleRequest", "setupProfile", "listProfiles", "inspectState", "advanceClock", "resetState", "exportCheckpoint", "restoreCheckpoint", "stepWork", "prepareWebhook", "executeWork", "acknowledgeWork", "retryWork"]);
