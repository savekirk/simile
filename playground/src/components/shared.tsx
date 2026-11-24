import type { ReactNode } from "react"
import { Field, FieldLabel, FieldDescription } from "./ui/field"
import { Input } from "./ui/input"
import { NativeSelect, NativeSelectOption } from "./ui/native-select"
import { Empty, EmptyHeader, EmptyTitle, EmptyDescription } from "./ui/empty"
import { Badge } from "./ui/badge"
export function TextField({
  label,
  value,
  onChange,
  hint,
  ...rest
}: {
  label: string
  value: string
  onChange: (value: string) => void
  hint?: string
  type?: string
  required?: boolean
  placeholder?: string
  disabled?: boolean
}) {
  const id = label.replaceAll(" ", "-")
  return (
    <Field>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      <Input
        id={id}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        {...rest}
      />
      {hint && <FieldDescription>{hint}</FieldDescription>}
    </Field>
  )
}
export function Choice({
  label,
  value,
  onChange,
  options,
  disabled,
}: {
  label: string
  value: string
  onChange: (value: string) => void
  options: { value: string; label: string; disabled?: boolean }[]
  disabled?: boolean
}) {
  const id = label.replaceAll(" ", "-")
  return (
    <Field>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      <NativeSelect
        id={id}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        disabled={disabled}
      >
        {options.map((option) => (
          <NativeSelectOption
            key={option.value}
            value={option.value}
            disabled={option.disabled}
          >
            {option.label}
          </NativeSelectOption>
        ))}
      </NativeSelect>
    </Field>
  )
}
export function NoArtifact() {
  return (
    <Empty>
      <EmptyHeader>
        <EmptyTitle>Compile your first revision</EmptyTitle>
        <EmptyDescription>
          Edit the definition and compile, or drop a compiled .similec artifact
          here.
        </EmptyDescription>
      </EmptyHeader>
    </Empty>
  )
}
export function Output({
  title,
  children,
}: {
  title: string
  children: ReactNode
}) {
  return (
    <section className="output">
      <div className="section-label">{title}</div>
      <pre tabIndex={0}>{children}</pre>
    </section>
  )
}
export function Method({ method }: { method: string }) {
  return (
    <Badge variant={method === "GET" ? "secondary" : "outline"}>{method}</Badge>
  )
}
export function PanelHeader({
  eyebrow,
  title,
  description,
  children,
}: {
  eyebrow?: string
  title: string
  description?: string
  children?: ReactNode
}) {
  return (
    <header className="panel-heading">
      <div>
        {eyebrow && <div className="eyebrow">{eyebrow}</div>}
        <h1>{title}</h1>
        {description && <p>{description}</p>}
      </div>
      <div className="flex items-center gap-2">{children}</div>
    </header>
  )
}

export function EmptyResult({ children }: { children: ReactNode }) {
  return (
    <Empty className="min-h-48">
      <EmptyHeader>
        <EmptyDescription>{children}</EmptyDescription>
      </EmptyHeader>
    </Empty>
  )
}
