import { useState, useSyncExternalStore, useRef } from "react"
import {
  BoxesIcon,
  FileCodeIcon,
  FolderOpenIcon,
  PlusIcon,
  DownloadIcon,
  UploadIcon,
  MoonIcon,
  SunIcon,
  ChevronDownIcon,
  BracesIcon,
  ChevronUpIcon,
  XIcon,
  BookOpenIcon,
} from "lucide-react"
import { Button } from "./components/ui/button"
import { Badge } from "./components/ui/badge"
import { Tabs, TabsList, TabsTrigger, TabsContent } from "./components/ui/tabs"
import { FieldGroup } from "./components/ui/field"
import { Separator } from "./components/ui/separator"
import { TooltipProvider } from "./components/ui/tooltip"
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
} from "./components/ui/dropdown-menu"
import {
  Sheet,
  SheetTrigger,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetDescription,
} from "./components/ui/sheet"
import { TextField, Method } from "./components/shared"
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from "./components/ui/resizable"
import { Author, Composition, Diagnostics } from "./views/author"
import { Inspect, Audit } from "./views/inspect"
import { Runner, Reproduction } from "./views/runner"
import { Guide } from "./views/guide"
import { Workbench } from "./views/workbench"
import { Playground } from "./model/controller"
import {
  starter,
  safePath,
  renameFile,
  workspace,
  type Project,
} from "./model/project"
import {
  download,
  unpack,
  runnableProject,
  workspaceArchive,
} from "./model/archive"
import { Engine } from "./worker/client"
import { useTheme } from "./components/theme-provider"
const model = new Playground()
void model.initialize()
const tools = [
  "Problems",
  "Composition",
  "Describe",
  "Audit",
  "Reproduction",
  "Guide",
]
const narrowScreen = window.matchMedia("(max-width: 900px)")
const subscribeScreen = (notify: () => void) => {
  narrowScreen.addEventListener("change", notify)
  return () => narrowScreen.removeEventListener("change", notify)
}
async function droppedFiles(
  transfer: DataTransfer
): Promise<{ path: string; file: File }[]> {
  const entries = [...transfer.items]
    .map((item) => item.webkitGetAsEntry?.())
    .filter((entry): entry is FileSystemEntry => Boolean(entry))
  async function walk(
    entry: FileSystemEntry
  ): Promise<{ path: string; file: File }[]> {
    if (entry.isFile)
      return new Promise((resolve, reject) =>
        (entry as FileSystemFileEntry).file(
          (file) =>
            resolve([{ path: entry.fullPath.replace(/^\//, ""), file }]),
          reject
        )
      )
    const reader = (entry as FileSystemDirectoryEntry).createReader()
    let found: FileSystemEntry[] = [],
      batch: FileSystemEntry[]
    do {
      batch = await new Promise((resolve, reject) =>
        reader.readEntries(resolve, reject)
      )
      found = found.concat(batch)
    } while (batch.length)
    return (await Promise.all(found.map(walk))).flat()
  }
  return entries.length
    ? (await Promise.all(entries.map(walk))).flat()
    : [...transfer.files].map((file) => ({ path: file.name, file }))
}
export default function App() {
  const state = useSyncExternalStore(model.subscribe, model.snapshot),
    { setTheme } = useTheme()
  const narrow = useSyncExternalStore(
    subscribeScreen,
    () => narrowScreen.matches
  )
  const [tool, setTool] = useState("Problems"),
    [toolOpen, setToolOpen] = useState(
      Boolean(state.error || state.diagnostic || state.storageError)
    ),
    [runView, setRunView] = useState("Run"),
    [runnerVersion, setRunnerVersion] = useState(0),
    [selected, setSelected] = useState("orders.yaml"),
    [filename, setFilename] = useState(""),
    [operation, setOperation] = useState("POST /orders"),
    [drag, setDrag] = useState(false)
  const upload = useRef<HTMLInputElement>(null),
    folder = useRef<HTMLInputElement>(null)
  const file =
    selected in state.project.files
      ? selected
      : (Object.keys(state.project.files)[0] ?? "")
  const issue = [state.diagnostic, state.error, state.storageError]
    .filter(Boolean)
    .join("\n")
  const [seenIssue, setSeenIssue] = useState(state.issueVersion)
  if (state.issueVersion !== seenIssue) {
    setSeenIssue(state.issueVersion)
    if (issue) {
      setTool("Problems")
      setToolOpen(true)
    }
  }
  const artifact = state.candidate
  async function importFiles(items: { path: string; file: File }[]) {
    if (!items.length) return
    if (items.length === 1 && items[0].path.endsWith(".similec")) {
      await model.importArtifact(
        new Uint8Array(await items[0].file.arrayBuffer())
      )
      setRunnerVersion((version) => version + 1)
      setTool("Describe")
      setToolOpen(true)
      return
    }
    const files: Record<string, string> = {}
    for (const { path, file } of items) {
      if (file.size > 32 * 1024 * 1024)
        throw new Error("Files must be smaller than 32 MiB.")
      if (path.endsWith(".zip")) {
        for (const [name, data] of Object.entries(
          unpack(new Uint8Array(await file.arrayBuffer()))
        ))
          if (!name.endsWith(".similec"))
            files[safePath(name)] = new TextDecoder("utf-8", {
              fatal: true,
            }).decode(data)
      } else if (!path.endsWith(".similec"))
        files[safePath(path)] = await file.text()
      else throw new Error("Import a compiled artifact by itself.")
    }
    const configs = Object.keys(files).filter((path) =>
      path.endsWith("Simfile")
    )
    const contracts = Object.keys(files).filter((path) =>
      /\.(yaml|yml|d\.ts)$/.test(path)
    )
    const project: Project = {
      name: "Imported workspace",
      files,
      notes: files["NOTES.md"] ?? "",
      modules: [],
      configPath: configs.length === 1 ? configs[0] : undefined,
      contractEntry: configs.length
        ? undefined
        : contracts.length === 1
          ? contracts[0]
          : undefined,
    }
    model.edit(project)
    setRunnerVersion((version) => version + 1)
    setSelected(
      project.configPath ?? project.contractEntry ?? Object.keys(files)[0]
    )
    setToolOpen(false)
  }
  const tree = (
    <>
      <div className="sidebar-heading">
        <span>
          {state.mode === "workspace" ? "PROJECT FILES" : "COMPILED ARTIFACT"}
        </span>
        <Badge variant="outline">
          {state.mode === "workspace"
            ? Object.keys(state.project.files).length
            : ".similec"}
        </Badge>
      </div>
      {state.mode === "workspace" ? (
        <>
          <nav aria-label="Project files" className="file-tree">
            {Object.keys(state.project.files)
              .sort()
              .map((path) => (
                <button
                  key={path}
                  aria-current={file === path ? "page" : undefined}
                  onClick={() => {
                    setSelected(path)
                    setToolOpen(false)
                    setFilename(path)
                  }}
                >
                  <FileCodeIcon aria-hidden="true" />
                  <span>{path}</span>
                </button>
              ))}
          </nav>
          <details className="file-actions">
            <summary>Manage files</summary>
            <FieldGroup>
              <TextField
                label="File path"
                value={filename}
                onChange={setFilename}
                placeholder="modules/behavior.hcl"
              />
            </FieldGroup>
            <div className="flex flex-wrap gap-1 pt-3">
              <Button
                variant="outline"
                size="sm"
                onClick={() =>
                  void model.act(async () => {
                    const path = safePath(filename)
                    if (path in state.project.files)
                      throw new Error("That file already exists.")
                    model.edit({
                      ...state.project,
                      files: { ...state.project.files, [path]: "" },
                    })
                    setSelected(path)
                  })
                }
              >
                <PlusIcon data-icon="inline-start" />
                Create
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() =>
                  void model.act(async () => {
                    model.edit(renameFile(state.project, file, filename))
                    setSelected(filename)
                  })
                }
              >
                Rename
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  const files = { ...state.project.files }
                  delete files[file]
                  model.edit({ ...state.project, files })
                }}
              >
                Delete selected
              </Button>
            </div>
            <p>
              Update imports and Simfile references after a rename or deletion.
            </p>
          </details>
        </>
      ) : (
        <p className="px-4 text-sm text-muted-foreground">
          Sources are unavailable. Import a workspace to author a service.
        </p>
      )}
      <Separator />
      <div className="sidebar-heading">
        <span>OPERATIONS</span>
        <Badge variant="outline">{artifact?.operations ?? 0}</Badge>
      </div>
      <nav aria-label="Operations" className="operation-tree">
        {artifact?.routes.map((route) => (
          <button
            key={route.key}
            onClick={() => {
              setOperation(route.key)
              setRunView("Run")
            }}
          >
            <div className="flex items-center gap-2">
              <Method method={route.method} />
              <code>{route.path}</code>
            </div>
            <span>{route.operationId ?? "unnamed operation"}</span>
            <div className="flex flex-wrap gap-1">
              {route.effects > 0 && <Badge variant="outline">effects</Badge>}
              {route.webhooks > 0 && <Badge variant="outline">webhooks</Badge>}
              {artifact.catalogue.operations.find((op) => op.key === route.key)
                ?.behaviorSource === "provider_authored" && (
                <Badge variant="outline">HCL</Badge>
              )}
              {artifact.catalogue.operations.find((op) => op.key === route.key)
                ?.secured && <Badge variant="outline">secured</Badge>}
            </div>
          </button>
        ))}
      </nav>
      <div className="sidebar-footer">
        <BracesIcon aria-hidden="true" />
        <p>
          Runs on your device.
          <br />
          No service traffic leaves this browser.
        </p>
      </div>
    </>
  )
  return (
    <TooltipProvider>
      <div
        className="app-shell"
        onDragOver={(event) => {
          event.preventDefault()
          setDrag(true)
        }}
        onDragLeave={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget as Node))
            setDrag(false)
        }}
        onDrop={(event) => {
          event.preventDefault()
          setDrag(false)
          const transfer = event.dataTransfer
          void model.act(async () => importFiles(await droppedFiles(transfer)))
        }}
      >
        {drag && (
          <div className="drop-overlay">
            Drop source files, a workspace ZIP, or one .similec artifact
          </div>
        )}
        <header className="app-header">
          <a className="brand" href="#" aria-label="Simile playground">
            <span className="brand-symbol">
              <BoxesIcon />
            </span>
            <strong>simile</strong>
            <span className="brand-divider">/</span>
            <span>playground</span>
          </a>
          <div className="header-actions">
            <Sheet>
              <SheetTrigger
                className="mobile-project"
                render={<Button variant="ghost" size="sm" />}
              >
                <FolderOpenIcon data-icon="inline-start" />
                Project
              </SheetTrigger>
              <SheetContent side="left">
                <SheetHeader>
                  <SheetTitle>Project explorer</SheetTitle>
                  <SheetDescription>
                    Workspace files and compiled operations.
                  </SheetDescription>
                </SheetHeader>
                {tree}
              </SheetContent>
            </Sheet>
            <DropdownMenu>
              <DropdownMenuTrigger
                aria-label="New workspace"
                render={<Button variant="ghost" size="sm" />}
              >
                <PlusIcon data-icon="inline-start" />
                <span className="workspace-label">New workspace</span>
                <span className="workspace-label-short">New</span>
                <ChevronDownIcon data-icon="inline-end" />
              </DropdownMenuTrigger>
              <DropdownMenuContent>
                <DropdownMenuGroup>
                  {[
                    ["orders", "Orders API"],
                    ["typescript", "Semantic TypeScript"],
                    ["blank", "Blank OpenAPI"],
                  ].map(([kind, label]) => (
                    <DropdownMenuItem
                      key={kind}
                      onClick={() => {
                        model.edit(starter(kind))
                        setRunnerVersion((version) => version + 1)
                        setToolOpen(false)
                      }}
                    >
                      {label}
                    </DropdownMenuItem>
                  ))}
                </DropdownMenuGroup>
              </DropdownMenuContent>
            </DropdownMenu>
            <DropdownMenu>
              <DropdownMenuTrigger
                render={<Button variant="ghost" size="sm" />}
              >
                <UploadIcon data-icon="inline-start" />
                Import
              </DropdownMenuTrigger>
              <DropdownMenuContent>
                <DropdownMenuGroup>
                  <DropdownMenuItem onClick={() => upload.current?.click()}>
                    Files, ZIP, or .similec
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => folder.current?.click()}>
                    Source folder
                  </DropdownMenuItem>
                </DropdownMenuGroup>
              </DropdownMenuContent>
            </DropdownMenu>
            <DropdownMenu>
              <DropdownMenuTrigger
                render={
                  <Button variant="ghost" size="sm" disabled={!artifact} />
                }
              >
                <DownloadIcon data-icon="inline-start" />
                Export
              </DropdownMenuTrigger>
              <DropdownMenuContent>
                <DropdownMenuGroup>
                  <DropdownMenuItem
                    disabled={!artifact}
                    onClick={() =>
                      artifact &&
                      download(
                        `${artifact.metadata?.service ?? artifact.service}.similec`,
                        artifact.bytes
                      )
                    }
                  >
                    Compiled artifact
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    disabled={state.mode !== "workspace" || !artifact}
                    onClick={() =>
                      void model.act(async () => {
                        const project = runnableProject(state.project),
                          compiler = new Engine()
                        try {
                          const checked = await compiler.call(
                            "compileWorkspace",
                            workspace(project)
                          )
                          download(
                            "workspace.zip",
                            workspaceArchive(project, checked)
                          )
                        } finally {
                          compiler.dispose()
                        }
                      })
                    }
                  >
                    Runnable workspace ZIP
                  </DropdownMenuItem>
                </DropdownMenuGroup>
              </DropdownMenuContent>
            </DropdownMenu>
            <div className="header-help">
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  setTool("Guide")
                  setToolOpen(true)
                }}
              >
                <BookOpenIcon data-icon="inline-start" />
                Guide
              </Button>
              <Button
                variant="ghost"
                size="icon"
                aria-label="Toggle color theme"
                onClick={() =>
                  setTheme(
                    document.documentElement.classList.contains("dark")
                      ? "light"
                      : "dark"
                  )
                }
              >
                <SunIcon className="hidden dark:block" />
                <MoonIcon className="dark:hidden" />
              </Button>
            </div>
            <input
              ref={upload}
              className="sr-only"
              tabIndex={-1}
              aria-label="Import files"
              type="file"
              multiple
              onChange={(event) => {
                const files = [...(event.target.files ?? [])]
                void model.act(() =>
                  importFiles(
                    files.map((file) => ({
                      path: file.webkitRelativePath || file.name,
                      file,
                    }))
                  )
                )
                event.target.value = ""
              }}
            />
            <input
              ref={(node) => {
                folder.current = node
                node?.setAttribute("webkitdirectory", "")
              }}
              className="sr-only"
              tabIndex={-1}
              aria-label="Import folder"
              type="file"
              multiple
              onChange={(event) => {
                const files = [...(event.target.files ?? [])]
                void model.act(() =>
                  importFiles(
                    files.map((file) => ({
                      path: file.webkitRelativePath || file.name,
                      file,
                    }))
                  )
                )
                event.target.value = ""
              }}
            />
          </div>
        </header>
        <div className="workspace-shell">
          <aside className="project-sidebar">{tree}</aside>
          <main className="main-panel">
            <ResizablePanelGroup
              orientation={narrow ? "vertical" : "horizontal"}
              className="studio-panels"
            >
              <ResizablePanel
                id="definition"
                defaultSize={narrow ? "45%" : "58%"}
                minSize={narrow ? "25%" : "32%"}
              >
                <div className="definition-pane">
                  <Author
                    model={model}
                    state={state}
                    file={file}
                    onSelectFile={setSelected}
                    onAddOperation={() => {
                      setTool("Guide")
                      setToolOpen(true)
                    }}
                  />
                  <Tabs
                    value={tool}
                    onValueChange={(value) => {
                      setTool(String(value))
                      setToolOpen(true)
                    }}
                    className={`tool-dock ${toolOpen ? "is-open" : ""}`}
                  >
                    <div className="dock-toolbar">
                      <div className="dock-tabs">
                        <TabsList variant="line" aria-label="Workspace tools">
                          {tools.map((name) => (
                            <TabsTrigger
                              key={name}
                              value={name}
                              onClick={() => setToolOpen(true)}
                            >
                              {name}
                              {name === "Problems" && issue && (
                                <span
                                  className="problem-dot"
                                  aria-hidden="true"
                                />
                              )}
                            </TabsTrigger>
                          ))}
                        </TabsList>
                      </div>
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        aria-label={
                          toolOpen ? "Collapse tools" : "Expand tools"
                        }
                        onClick={() => setToolOpen(!toolOpen)}
                      >
                        {toolOpen ? <XIcon /> : <ChevronUpIcon />}
                      </Button>
                    </div>
                    <div className="tool-body" hidden={!toolOpen}>
                      <TabsContent value="Guide" keepMounted>
                        <Guide
                          state={state}
                          onOpenFile={setSelected}
                          onRun={() => setRunView("Run")}
                          onHttp={() => setRunView("HTTP")}
                        />
                      </TabsContent>
                      <TabsContent value="Problems" keepMounted>
                        <Diagnostics
                          model={model}
                          state={state}
                          onSelectFile={setSelected}
                        />
                      </TabsContent>
                      <TabsContent value="Composition" keepMounted>
                        <Composition model={model} state={state} />
                      </TabsContent>
                      <TabsContent value="Describe" keepMounted>
                        <Inspect state={state} />
                      </TabsContent>
                      <TabsContent value="Audit" keepMounted>
                        <Audit state={state} />
                      </TabsContent>
                      <TabsContent value="Reproduction" keepMounted>
                        <Reproduction model={model} state={state} />
                      </TabsContent>
                    </div>
                  </Tabs>
                </div>
              </ResizablePanel>
              <ResizableHandle aria-label="Resize definition and execution" />
              <ResizablePanel
                id="execution"
                defaultSize={narrow ? "55%" : "42%"}
                minSize={narrow ? "30%" : "30%"}
              >
                <Tabs
                  value={runView}
                  onValueChange={(value) => setRunView(String(value))}
                  className="execution-pane"
                >
                  <div className="execution-tabs">
                    <TabsList variant="line" aria-label="Execution mode">
                      <TabsTrigger value="Run">Run</TabsTrigger>
                      <TabsTrigger value="HTTP">HTTP session</TabsTrigger>
                    </TabsList>
                    <span className="execution-caption">
                      {runView === "Run"
                        ? "Fresh state each run"
                        : state.session
                          ? "Service running"
                          : "Stateful requests"}
                    </span>
                  </div>
                  <TabsContent
                    value="Run"
                    keepMounted
                    className="execution-content"
                  >
                    {state.projectState !== "loading" && (
                      <Runner
                        key={runnerVersion}
                        model={model}
                        state={state}
                        operation={operation}
                        onOperation={setOperation}
                      />
                    )}
                  </TabsContent>
                  <TabsContent
                    value="HTTP"
                    keepMounted
                    className="execution-content"
                  >
                    <Workbench model={model} state={state} />
                  </TabsContent>
                </Tabs>
              </ResizablePanel>
            </ResizablePanelGroup>
          </main>
        </div>
        <div className="revision-bar">
          <div className="flex flex-wrap items-center gap-2">
            <Badge variant={state.diagnostic ? "destructive" : "secondary"}>
              {state.compiling
                ? "Compiling…"
                : state.diagnostic
                  ? "Compile error"
                  : artifact
                    ? `Compiled · ${artifact.operations} ops · ${artifact.revision.slice(0, 8)}`
                    : "Not compiled"}
            </Badge>
            {state.mode === "workspace" &&
              state.compiledRevision !== state.editRevision &&
              artifact && <Badge variant="outline">Source edits pending</Badge>}
            {artifact && (
              <Badge
                variant={
                  artifact.audit.errors || artifact.audit.warnings
                    ? "outline"
                    : "secondary"
                }
              >
                Strict audit:{" "}
                {artifact.audit.errors || artifact.audit.warnings
                  ? "findings"
                  : "pass"}
              </Badge>
            )}
          </div>
          <span role="status">
            Project {state.projectState}
            {state.busy ? " · Working…" : ""}
          </span>
        </div>
      </div>
    </TooltipProvider>
  )
}
