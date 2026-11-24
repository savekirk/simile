import { useState } from "react"
import {
  PlayIcon,
  RotateCcwIcon,
  SaveIcon,
  ClockIcon,
  SendIcon,
} from "lucide-react"
import { Button } from "../components/ui/button"
import { Badge } from "../components/ui/badge"
import { Field, FieldGroup, FieldLabel } from "../components/ui/field"
import { Input } from "../components/ui/input"
import { Switch } from "../components/ui/switch"
import { Tabs, TabsList, TabsTrigger, TabsContent } from "../components/ui/tabs"
import { ToggleGroup, ToggleGroupItem } from "../components/ui/toggle-group"
import { Alert, AlertTitle, AlertDescription } from "../components/ui/alert"
import { Editor } from "../components/editor"
import {
  EmptyResult,
  PanelHeader,
  Choice,
  TextField,
  Output,
} from "../components/shared"
import type { Playground, State } from "../model/controller"
import type {
  HttpExchange,
  ProfileSummary,
  SessionOptions,
  StateView,
  WorkInspection,
  WorkItem,
} from "../worker/client"
import { download } from "../model/archive"
import { setupValues } from "../model/setup"
const encode = (text: string) => new TextEncoder().encode(text),
  decode = (bytes: Uint8Array) => new TextDecoder().decode(bytes)
function wireText(bytes: Uint8Array): string {
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes)
  } catch {
    return (
      "hex: " +
      Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join(" ")
    )
  }
}
export function Workbench({
  model,
  state,
}: {
  model: Playground
  state: State
}) {
  const [options, setOptions] = useState<SessionOptions>({
      clockMode: "manual",
    }),
    [advance, setAdvance] = useState("1000")
  const [tab, setTab] = useState("request")
  return (
    <>
      <PanelHeader
        eyebrow="In-process HTTP · no listening socket"
        title="Stateful workbench"
        description="Start a compiled revision explicitly. Controls apply on restart and reset the running state."
      >
        <Badge variant="secondary">
          {state.session
            ? `Running ${state.active?.revision.slice(0, 8)}`
            : "Stopped"}
        </Badge>
      </PanelHeader>
      <FieldGroup className="session-controls">
        <Field>
          <FieldLabel>Clock mode</FieldLabel>
          <ToggleGroup
            value={[options.clockMode ?? "manual"]}
            onValueChange={(values) => {
              if (values[0])
                setOptions({
                  ...options,
                  clockMode: values[0] as "manual" | "realtime",
                })
            }}
          >
            <ToggleGroupItem value="manual">Manual</ToggleGroupItem>
            <ToggleGroupItem value="realtime">Realtime</ToggleGroupItem>
          </ToggleGroup>
        </Field>
        <Choice
          label="Session scenario"
          value={options.scenario ?? ""}
          onChange={(scenario) =>
            setOptions({ ...options, scenario: scenario || undefined })
          }
          options={[
            { value: "", label: "Default" },
            ...(state.candidate?.catalogue.scenarios ?? []).map((value) => ({
              value: value.name,
              label: value.name,
            })),
          ]}
        />
        <Choice
          label="Fault profile"
          value={options.faultProfile ?? ""}
          onChange={(faultProfile) =>
            setOptions({ ...options, faultProfile: faultProfile || undefined })
          }
          options={[
            { value: "", label: "None" },
            ...(state.candidate?.catalogue.faultProfiles ?? []).map(
              (value) => ({ value: value.name, label: value.name })
            ),
          ]}
        />
        <Button
          disabled={state.busy || !state.candidate}
          onClick={() => void model.act(() => model.start(options))}
        >
          <PlayIcon data-icon="inline-start" />
          {state.session ? "Restart with controls" : "Start service"}
        </Button>
      </FieldGroup>
      <div className="service-toolbar">
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant="outline">
            {state.session?.clockMode ?? "manual"} ·{" "}
            {state.session?.clockMilliseconds ?? "0"} ms
          </Badge>
          <Input
            aria-label="Advance milliseconds"
            className="w-28"
            value={advance}
            onChange={(e) => setAdvance(e.target.value)}
          />
          <Button
            variant="outline"
            disabled={
              !state.session ||
              state.busy ||
              state.session.clockMode !== "manual"
            }
            onClick={() =>
              void model.act(() =>
                model.call("advanceClock", { milliseconds: advance })
              )
            }
          >
            <ClockIcon data-icon="inline-start" />
            Advance
          </Button>
          <Button
            variant="outline"
            disabled={!state.session || state.busy}
            onClick={() => void model.act(() => model.call("resetState", {}))}
          >
            <RotateCcwIcon data-icon="inline-start" />
            Reset state
          </Button>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant="secondary">Checkpoint {state.checkpointState}</Badge>
          <Button
            variant="outline"
            disabled={!state.session || state.busy}
            onClick={() => void model.act(() => model.saveCheckpoint())}
          >
            <SaveIcon data-icon="inline-start" />
            Save checkpoint
          </Button>
          <Button
            variant="ghost"
            disabled={state.busy}
            onClick={() => void model.act(() => model.restoreSaved())}
          >
            Restore saved
          </Button>
          <Button
            variant="ghost"
            disabled={!state.session || state.busy}
            onClick={() =>
              void model.act(async () =>
                download(
                  "service.checkpoint.json",
                  (await model.call("exportCheckpoint", {})).bytes
                )
              )
            }
          >
            Export
          </Button>
          <label className="file-picker small">
            Import checkpoint
            <input
              type="file"
              aria-label="Import checkpoint"
              accept=".json"
              disabled={!state.session || state.busy}
              onChange={(event) => {
                const file = event.target.files?.[0]
                if (file)
                  void model.act(async () =>
                    model.restoreBytes(new Uint8Array(await file.arrayBuffer()))
                  )
                event.target.value = ""
              }}
            />
          </label>
        </div>
      </div>
      {state.active && state.candidate?.revision !== state.active.revision && (
        <Alert>
          <AlertTitle>A different revision is running</AlertTitle>
          <AlertDescription>
            The running service keeps its artifact and state until you
            explicitly restart it.
          </AlertDescription>
        </Alert>
      )}
      <p className="text-sm text-muted-foreground">
        Restore preserves resources, profiles, and outbox. Replay caches and
        fault progress restart; old work handles become invalid.
      </p>
      {state.session && state.active ? (
        <Tabs
          value={tab}
          onValueChange={(value) => setTab(String(value))}
          className="mt-5"
        >
          <div className="min-w-0 overflow-x-auto pb-2">
            <TabsList variant="line">
              <TabsTrigger value="request">HTTP request</TabsTrigger>
              <TabsTrigger value="profiles">Profiles & setup</TabsTrigger>
              <TabsTrigger value="state">Stack & journal</TabsTrigger>
              <TabsTrigger value="work">Work & webhooks</TabsTrigger>
            </TabsList>
          </div>
          <TabsContent value="request">
            <RequestPanel
              key={state.active.revision}
              model={model}
              state={state}
            />
          </TabsContent>
          <TabsContent value="profiles">
            <Profiles model={model} state={state} />
          </TabsContent>
          <TabsContent value="state">
            <StackPanel model={model} state={state} />
          </TabsContent>
          <TabsContent value="work">
            <WorkPanel model={model} state={state} />
          </TabsContent>
        </Tabs>
      ) : (
        <EmptyResult>
          Start the compiled service, or restore a saved checkpoint to continue.
        </EmptyResult>
      )}
      <details className="mt-5">
        <summary>Engine recovery</summary>
        <p>
          Restarting the runtime Worker discards unsaved runtime state. Source
          edits and completed saves remain available.
        </p>
        <Button variant="destructive" onClick={() => model.restartRuntime()}>
          Restart runtime Worker
        </Button>
      </details>
    </>
  )
}
function RequestPanel({ model, state }: { model: Playground; state: State }) {
  const artifact = state.active!
  const [routeKey, setRouteKey] = useState(
    artifact.routes.find((route) => route.operationId === "createOrder")?.key ??
      artifact.routes[0]?.key ??
      ""
  )
  const route = artifact.routes.find((route) => route.key === routeKey)
  const [uri, setUri] = useState(route?.path ?? "/"),
    [method, setMethod] = useState(route?.method ?? "GET")
  const [params, setParams] = useState<Record<string, string>>({})
  const [headers, setHeaders] = useState<[string, string][]>([
    ["Content-Type", "application/json"],
  ])
  const [body, setBody] = useState('{"product":"licensed_beat","quantity":2}'),
    [mode, setMode] = useState("text"),
    [binary, setBinary] = useState<Uint8Array>()
  const [seed, setSeed] = useState("7"),
    [response, setResponse] = useState<HttpExchange>(),
    [elapsed, setElapsed] = useState(0)
  const [profile, setProfile] = useState(""),
    [useCredentials, setUseCredentials] = useState(false),
    [profiles, setProfiles] = useState<ProfileSummary[]>([])
  const request = () => {
    let target = uri
    const pairs = headers
      .filter(([name]) => name)
      .map(([name, value]) => [name, encode(value)] as [string, Uint8Array])
    const query: string[] = []
    const cookies: string[] = []
    for (const parameter of route?.parameters ?? []) {
      const value = params[`${parameter.location}:${parameter.name}`]
      if (value === undefined || value === "") continue
      if (parameter.location === "path")
        target = target.replaceAll(
          `{${parameter.name}}`,
          encodeURIComponent(value)
        )
      if (parameter.location === "query")
        query.push(
          `${encodeURIComponent(parameter.name)}=${encodeURIComponent(value)}`
        )
      if (parameter.location === "header")
        pairs.push([parameter.name, encode(value)])
      if (parameter.location === "cookie")
        cookies.push(`${parameter.name}=${encodeURIComponent(value)}`)
    }
    if (query.length)
      target += (target.includes("?") ? "&" : "?") + query.join("&")
    if (cookies.length) pairs.push(["Cookie", encode(cookies.join("; "))])
    return {
      method,
      uri: target,
      headers: pairs,
      ...(mode === "absent"
        ? {}
        : { body: mode === "file" ? binary : encode(body) }),
      responseSeed: seed || undefined,
      profile: useCredentials && profile ? profile : undefined,
    }
  }
  return (
    <div className="split-panels">
      <section>
        <FieldGroup>
          <Choice
            label="HTTP route"
            value={routeKey}
            onChange={(key) => {
              const route = artifact.routes.find((item) => item.key === key)!
              setRouteKey(key)
              setUri(route.path)
              setMethod(route.method)
              setMode(
                ["GET", "HEAD"].includes(route.method) ? "absent" : "text"
              )
              setParams({})
            }}
            options={artifact.routes.map((route) => ({
              value: route.key,
              label: route.key,
            }))}
          />
          <div className="grid grid-cols-[6rem_1fr] gap-3">
            <TextField label="Method" value={method} onChange={setMethod} />
            <TextField label="Request URI" value={uri} onChange={setUri} />
          </div>
          {route?.parameters.map((parameter) => (
            <TextField
              key={`${parameter.location}:${parameter.name}`}
              label={`${parameter.location}: ${parameter.name}`}
              hint={`${parameter.required ? "Required" : "Optional"} · ${parameter.style}${parameter.explode ? ", explode" : ""}. Enter a value or edit the URI/headers directly for complex serialization.`}
              value={params[`${parameter.location}:${parameter.name}`] ?? ""}
              onChange={(value) =>
                setParams({
                  ...params,
                  [`${parameter.location}:${parameter.name}`]: value,
                })
              }
            />
          ))}
          <Field>
            <FieldLabel>Repeated headers</FieldLabel>
            {headers.map(([name, value], index) => (
              <div className="flex gap-2" key={index}>
                <Input
                  aria-label={`Header ${index + 1} name`}
                  value={name}
                  onChange={(e) =>
                    setHeaders(
                      headers.map((row, i) =>
                        i === index ? [e.target.value, row[1]] : row
                      )
                    )
                  }
                />
                <Input
                  aria-label={`Header ${index + 1} value`}
                  value={value}
                  onChange={(e) =>
                    setHeaders(
                      headers.map((row, i) =>
                        i === index ? [row[0], e.target.value] : row
                      )
                    )
                  }
                />
                <Button
                  variant="ghost"
                  aria-label={`Remove header ${index + 1}`}
                  onClick={() =>
                    setHeaders(headers.filter((_, i) => i !== index))
                  }
                >
                  ×
                </Button>
              </div>
            ))}
            <Button
              variant="outline"
              onClick={() => setHeaders([...headers, ["", ""]])}
            >
              Add header
            </Button>
          </Field>
          <Field>
            <FieldLabel>Request body</FieldLabel>
            <ToggleGroup
              value={[mode]}
              onValueChange={(values) => {
                if (values[0]) setMode(values[0])
              }}
            >
              <ToggleGroupItem value="absent">Absent</ToggleGroupItem>
              <ToggleGroupItem value="text">Text / JSON</ToggleGroupItem>
              <ToggleGroupItem value="file">Binary file</ToggleGroupItem>
            </ToggleGroup>
          </Field>
          {mode === "text" && (
            <div className="editor-panel">
              <Editor path="http-body.json" value={body} onChange={setBody} />
            </div>
          )}
          {mode === "file" && (
            <Input
              aria-label="Binary request body"
              type="file"
              onChange={(event) => {
                const file = event.target.files?.[0]
                if (file)
                  void file
                    .arrayBuffer()
                    .then((buffer) => setBinary(new Uint8Array(buffer)))
              }}
            />
          )}
          <TextField label="Response seed" value={seed} onChange={setSeed} />
          <Choice
            label="Credential profile"
            value={profile}
            onChange={setProfile}
            options={[
              { value: "", label: "Choose a profile" },
              ...profiles.map((value) => ({
                value: value.name,
                label: `${value.name} · ${value.principal}`,
              })),
            ]}
          />
          <Field orientation="horizontal">
            <Switch
              id="use-credentials"
              checked={useCredentials}
              onCheckedChange={setUseCredentials}
            />
            <FieldLabel htmlFor="use-credentials">
              Use profile credentials
            </FieldLabel>
            <Button
              variant="ghost"
              disabled={state.busy}
              onClick={() =>
                void model.act(async () => {
                  const values = await model.call("listProfiles", {})
                  setProfiles(values)
                  setProfile(
                    values.find((value) => value.current)?.name ??
                      values[0]?.name ??
                      ""
                  )
                })
              }
            >
              Refresh profiles
            </Button>
          </Field>
        </FieldGroup>
        <div className="flex flex-wrap gap-2 py-4">
          <Button
            disabled={state.busy || (mode === "file" && !binary)}
            onClick={() =>
              void model.act(async () => {
                const start = performance.now()
                const result = await model.call("handleRequest", request())
                setElapsed(performance.now() - start)
                setResponse(result)
              })
            }
          >
            <SendIcon data-icon="inline-start" />
            Send request
          </Button>
          <Button
            variant="outline"
            disabled={state.busy || !route}
            onClick={() =>
              void model.act(async () => {
                const generated = await model.call("synthesizeRequest", {
                  revision: artifact.revision,
                  options: {
                    selector: { key: route!.key },
                    seed: seed || undefined,
                  },
                })
                setMode(generated.body ? "text" : "absent")
                setBody(generated.body ? decode(generated.body) : "")
                if (generated.contentType)
                  setHeaders([["Content-Type", generated.contentType]])
              })
            }
          >
            Synthesize body
          </Button>
        </div>
        <details>
          <summary>Native curl handoff</summary>
          <p>
            After native registration and required setup; attach the credentials
            issued there.
          </p>
          <Output title="Local terminal">{`curl --request ${method} '${artifact.metadata?.listen ? "http://" + artifact.metadata.listen : "http://127.0.0.1:4242"}${uri}'\n# Supply headers and body from this workbench after replacing path parameters.`}</Output>
        </details>
      </section>
      <section>
        {response ? (
          <>
            <div className="response-status">
              <Badge>{response.status}</Badge>
              <span>{elapsed.toFixed(1)} ms engine time</span>
              <span>seed {response.seed}</span>
            </div>
            {response.transport.kind !== "respond" && (
              <Alert>
                <AlertTitle>
                  {response.transport.kind === "delay"
                    ? `Simulated delay: ${response.transport.milliseconds} ms`
                    : "Simulated disconnect after commit"}
                </AlertTitle>
                <AlertDescription>
                  {response.transport.kind === "delay"
                    ? "The transport plan delays delivery on a native listener. This workbench opens no socket."
                    : "Effects were committed. A native client would receive a disconnect; the body below is diagnostic."}
                </AlertDescription>
              </Alert>
            )}
            <Output title="Response headers">
              {response.headers
                .map(([name, value]) => `${name}: ${wireText(value)}`)
                .join("\n")}
            </Output>
            <Output title="Response body · exact text / hex">
              {wireText(response.body) || "(empty)"}
            </Output>
            <Button
              variant="outline"
              onClick={() => download("response.bin", response.body)}
            >
              Download exact bytes
            </Button>
            {response.incident && (
              <Output title="Incident">{response.incident.message}</Output>
            )}
          </>
        ) : (
          <EmptyResult>
            Send a request to inspect its HTTP result and transport plan.
          </EmptyResult>
        )}
      </section>
    </div>
  )
}
function Profiles({ model, state }: { model: Playground; state: State }) {
  const setups = state.active!.catalogue.principalSetups
  const [principal, setPrincipal] = useState(setups[0]?.principal ?? ""),
    [name, setName] = useState("playground"),
    [values, setValues] = useState<Record<string, string>>({})
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]),
    [profileJson, setProfileJson] = useState(""),
    [reveal, setReveal] = useState(false)
  const setup = setups.find((item) => item.principal === principal)
  return (
    <div className="split-panels">
      <section>
        <h2 className="mb-4">Provision a caller</h2>
        <FieldGroup>
          <Choice
            label="Principal setup"
            value={principal}
            onChange={(value) => {
              setPrincipal(value)
              setValues({})
            }}
            options={setups.map((value) => ({
              value: value.principal,
              label: value.principal,
            }))}
          />
          <TextField label="Profile name" value={name} onChange={setName} />
          {setup?.collects.map((field) => (
            <TextField
              key={field.name}
              label={field.name}
              value={values[field.name] ?? ""}
              onChange={(value) =>
                setValues({ ...values, [field.name]: value })
              }
              required={field.required}
              hint={`${field.description ?? ""} (${field.valueType})`}
            />
          ))}
        </FieldGroup>
        <Button
          className="mt-4"
          disabled={!setup || state.busy}
          onClick={() =>
            void model.act(async () => {
              const result = await model.call("setupProfile", {
                principal,
                name,
                valuesJson: setupValues(setup!.collects, values),
                select: true,
                seed: "7",
              })
              setProfileJson(result.profileJson)
              setReveal(false)
              setProfiles(await model.call("listProfiles", {}))
            })
          }
        >
          Create profile
        </Button>
        {!setup && (
          <p>This service does not declare a principal setup workflow.</p>
        )}
        {profileJson && (
          <div className="mt-4">
            <Button variant="outline" onClick={() => setReveal(!reveal)}>
              {reveal ? "Hide issued credentials" : "Reveal issued credentials"}
            </Button>
            {reveal ? (
              <Output title="Issued profile · sensitive values">
                {profileJson}
              </Output>
            ) : (
              <p className="mt-3">
                Credentials issued. Use the profile toggle in the HTTP request
                panel to attach them.
              </p>
            )}
          </div>
        )}
      </section>
      <section>
        <div className="flex justify-between gap-3">
          <h2>Available profiles</h2>
          <Button
            variant="outline"
            disabled={state.busy}
            onClick={() =>
              void model.act(async () =>
                setProfiles(await model.call("listProfiles", {}))
              )
            }
          >
            Refresh
          </Button>
        </div>
        {profiles.map((profile) => (
          <div className="case-row" key={profile.id}>
            <div>
              <strong>{profile.name}</strong>
              <p>
                {profile.principal} · {profile.status}
              </p>
              <p>
                {profile.issued
                  .map(
                    (field) =>
                      `${field.name}: ${field.sensitive ? "••••••" : "issued"}`
                  )
                  .join(", ")}
              </p>
            </div>
            <Badge variant="secondary">
              {profile.current ? "Current" : "Available"}
            </Badge>
          </div>
        ))}
      </section>
    </div>
  )
}
function StackPanel({ model, state }: { model: Playground; state: State }) {
  const [resource, setResource] = useState(""),
    [correlationId, setCorrelation] = useState(""),
    [result, setResult] = useState<StateView>()
  const resources = state.active!.catalogue.backends.map(
    (item) => item.reference
  )
  return (
    <>
      <FieldGroup className="filters">
        <Choice
          label="Backend resource"
          value={resource}
          onChange={setResource}
          options={[
            { value: "", label: "All resources (journal)" },
            ...resources.map((value) => ({ value, label: value })),
          ]}
        />
        <TextField
          label="Correlation ID"
          value={correlationId}
          onChange={setCorrelation}
        />
        <Button
          disabled={state.busy}
          onClick={() =>
            void model.act(async () =>
              setResult(
                await model.call("inspectState", {
                  resource: resource || undefined,
                  correlationId: correlationId || undefined,
                  includeValues: Boolean(resource),
                  limit: 200,
                })
              )
            )
          }
        >
          Inspect snapshot
        </Button>
      </FieldGroup>
      <p className="text-sm text-muted-foreground">
        Inspection does not mutate the live Stack. Choose a backend to include
        its values.
      </p>
      {result && (
        <Output
          title={`${result.totalEvents} journal events · ${result.resources.length} resources`}
        >
          {result.reportJson}
        </Output>
      )}
    </>
  )
}
function WorkPanel({ model, state }: { model: Playground; state: State }) {
  const [items, setItems] = useState<WorkInspection[]>([]),
    [leases, setLeases] = useState<WorkItem[]>([]),
    [prepared, setPrepared] = useState("")
  const [timestamp, setTimestamp] = useState("0"),
    [delay, setDelay] = useState("1000")
  const inspect = async () => setItems(await model.call("inspectWork", {}))
  return (
    <>
      <div className="flex flex-wrap gap-2 py-4">
        <Button
          variant="outline"
          disabled={state.busy}
          onClick={() => void model.act(inspect)}
        >
          Inspect outbox
        </Button>
        <Button
          disabled={state.busy}
          onClick={() =>
            void model.act(async () => {
              const work = await model.call("stepWork", { limit: 20 })
              setLeases((previous) => [
                ...previous.filter(
                  (item) => !work.some((next) => next.id === item.id)
                ),
                ...work,
              ])
              await inspect()
            })
          }
        >
          Step work
        </Button>
        <Button
          variant="outline"
          disabled={state.busy}
          onClick={() =>
            void model.act(async () => {
              await model.call("redeliverDeadLetter", {})
              await inspect()
            })
          }
        >
          Redeliver next DLQ item
        </Button>
      </div>
      <p className="text-sm text-muted-foreground">
        Inspect is read-only. Step leases causally ready items for 30 seconds of
        service time. Webhook outcomes simulate a recipient; no network delivery
        occurs.
      </p>
      <FieldGroup className="filters">
        <TextField
          label="Signing timestamp"
          value={timestamp}
          onChange={setTimestamp}
        />
        <TextField label="Retry delay (ms)" value={delay} onChange={setDelay} />
      </FieldGroup>
      {items.map((item) => (
        <div className="work-card" key={item.id}>
          <div className="flex flex-wrap items-center gap-2">
            <h2>{item.name}</h2>
            <Badge variant="outline">{item.kind}</Badge>
            <Badge variant="secondary">
              {item.deadLetter
                ? "Dead letter"
                : item.leased
                  ? "Leased"
                  : item.readiness}
            </Badge>
            <span className="text-sm text-muted-foreground">
              Available at {item.availableAt} ms
            </span>
          </div>
          <details>
            <summary>Work payload</summary>
            <Output title="Exact work JSON">{item.workJson}</Output>
          </details>
        </div>
      ))}
      {leases.map((item) => (
        <div className="case-row" key={item.handle}>
          <div>
            <strong>{item.name}</strong>
            <p>
              Attempt {item.attempt} · {item.id}
            </p>
          </div>
          {item.kind === "async" ? (
            <Button
              disabled={state.busy}
              onClick={() =>
                void model.act(async () => {
                  await model.call("executeWork", { handle: item.handle })
                  setLeases(
                    leases.filter((lease) => lease.handle !== item.handle)
                  )
                  await inspect()
                })
              }
            >
              Execute async
            </Button>
          ) : (
            <>
              <Button
                variant="outline"
                disabled={state.busy}
                onClick={() =>
                  void model.act(async () => {
                    const result = await model.call("prepareWebhook", {
                      handle: item.handle,
                      timestamp,
                    })
                    setPrepared(
                      `${result.method} ${result.uri}\n${result.headers.map(([name, value]) => `${name}: ${wireText(value)}`).join("\n")}\n\n${decode(result.body)}`
                    )
                  })
                }
              >
                Prepare webhook
              </Button>
              <Button
                disabled={state.busy}
                onClick={() =>
                  void model.act(async () => {
                    await model.call("acknowledgeWork", { handle: item.handle })
                    setLeases(
                      leases.filter((lease) => lease.handle !== item.handle)
                    )
                    await inspect()
                  })
                }
              >
                Simulate success
              </Button>
            </>
          )}
          <Button
            variant="outline"
            disabled={state.busy}
            onClick={() =>
              void model.act(async () => {
                await model.call("retryWork", { handle: item.handle, delay })
                setLeases(
                  leases.filter((lease) => lease.handle !== item.handle)
                )
                await inspect()
              })
            }
          >
            Simulate failure
          </Button>
        </div>
      ))}
      {prepared && (
        <Output title="Prepared webhook · simulated delivery only">
          {prepared}
        </Output>
      )}
    </>
  )
}
