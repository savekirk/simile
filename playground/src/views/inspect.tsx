import { useState } from "react"
import {
  PanelHeader,
  NoArtifact,
  Method,
  Output,
  Choice,
  TextField,
} from "../components/shared"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "../components/ui/table"
import { Badge } from "../components/ui/badge"
import { Field, FieldGroup, FieldLabel } from "../components/ui/field"
import { Switch } from "../components/ui/switch"
import type { State } from "../model/controller"
export function Inspect({ state }: { state: State }) {
  const artifact = state.candidate
  if (!artifact) return <NoArtifact />
  return (
    <>
      <PanelHeader
        eyebrow="Public catalogue"
        title={artifact.service}
        description={
          artifact.catalogue.description ??
          "The operations and behavior described by this compiled revision."
        }
      />
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Operation</TableHead>
            <TableHead>Contract</TableHead>
            <TableHead>Outcomes</TableHead>
            <TableHead>Access</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {artifact.catalogue.operations.map((op) => (
            <TableRow key={op.key}>
              <TableCell>
                <div className="flex items-center gap-2">
                  <Method method={op.key.split(" ")[0]} />
                  <code>{op.key.slice(op.key.indexOf(" ") + 1)}</code>
                </div>
                <div className="mt-1 text-muted-foreground">
                  {op.operationId}
                </div>
              </TableCell>
              <TableCell>{op.description ?? op.behaviorSource}</TableCell>
              <TableCell>
                {op.outcomes.map((outcome) => (
                  <Badge key={outcome.name} variant="secondary">
                    {outcome.status} · {outcome.name}
                  </Badge>
                ))}
              </TableCell>
              <TableCell>
                {op.secured ? "Profile required" : "Public"}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
      <div className="catalogue-sections">
        {Object.entries(artifact.catalogue)
          .filter(
            ([key]) =>
              ![
                "operations",
                "service",
                "serviceVersion",
                "openapiVersion",
                "operationCount",
                "description",
              ].includes(key)
          )
          .map(([key, value]) => (
            <details key={key}>
              <summary>
                {key.replace(/[A-Z]/g, (letter) => " " + letter.toLowerCase())}
              </summary>
              <Output title={key}>{JSON.stringify(value, null, 2)}</Output>
            </details>
          ))}
      </div>
    </>
  )
}
export function Audit({ state }: { state: State }) {
  const [strict, setStrict] = useState(false),
    [severity, setSeverity] = useState("all"),
    [query, setQuery] = useState("")
  const artifact = state.candidate
  if (!artifact) return <NoArtifact />
  const report = artifact.audit,
    accepted = report.errors === 0 && (!strict || report.warnings === 0)
  const findings = report.findings.filter(
    (finding) =>
      (severity === "all" || finding.severity === severity) &&
      `${finding.code} ${finding.subject} ${finding.message}`
        .toLowerCase()
        .includes(query.toLowerCase())
  )
  return (
    <>
      <PanelHeader
        eyebrow="Fidelity"
        title="Audit the contract"
        description="Decide which inferred behavior and findings your service can accept."
      >
        <Badge variant={accepted ? "secondary" : "destructive"}>
          {accepted ? "Accepted" : "Not accepted"}
        </Badge>
      </PanelHeader>
      <dl className="audit-metrics">
        <div>
          <dt>Errors</dt>
          <dd>{report.errors}</dd>
        </div>
        <div>
          <dt>Warnings</dt>
          <dd>{report.warnings}</dd>
        </div>
        <div>
          <dt>Provider effects</dt>
          <dd>{report.providerAuthoredEffects}</dd>
        </div>
        <div>
          <dt>Inferred effects</dt>
          <dd>{report.conventionInferredEffects}</dd>
        </div>
      </dl>
      <FieldGroup className="filters">
        <TextField label="Filter findings" value={query} onChange={setQuery} />
        <Choice
          label="Severity"
          value={severity}
          onChange={setSeverity}
          options={["all", "error", "warning", "info"].map((value) => ({
            value,
            label: value,
          }))}
        />
        <Field orientation="horizontal">
          <Switch
            id="strict-audit"
            checked={strict}
            onCheckedChange={setStrict}
          />
          <FieldLabel htmlFor="strict-audit">Strict acceptance</FieldLabel>
        </Field>
      </FieldGroup>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Severity / code</TableHead>
            <TableHead>Subject</TableHead>
            <TableHead>Finding and remediation</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {findings.map((finding, i) => (
            <TableRow key={i}>
              <TableCell>
                <Badge
                  variant={
                    finding.severity === "error" ? "destructive" : "secondary"
                  }
                >
                  {finding.severity}
                </Badge>
                <div className="mt-2 font-mono text-xs">{finding.code}</div>
              </TableCell>
              <TableCell>{finding.subject}</TableCell>
              <TableCell>
                <p>{finding.message}</p>
                <p className="mt-1 text-muted-foreground">
                  {finding.remediation}
                </p>
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
      {!findings.length && (
        <p className="p-6 text-muted-foreground">
          No findings match this filter.
        </p>
      )}
    </>
  )
}
