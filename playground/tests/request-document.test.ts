import { expect, test } from "vitest"
import {
  hasRequestBody,
  withoutRequestBody,
} from "../src/model/request-document"

test("body detection distinguishes absence, null, escaped names, and nested fields", () => {
  for (const source of ['{"body":null}', '{"bo\\u0064y":{}}', '{"body":false}'])
    expect(hasRequestBody(source)).toBe(true)
  for (const source of [
    "{}",
    '{"headers":{"body":"kept"}}',
    '{"body":',
    "null",
  ])
    expect(hasRequestBody(source)).toBe(false)
})

test("removing body preserves exact remaining fields, integer literals, and header pairs", () => {
  const remaining =
    '"headers": [["x", "a"], ["x", "b"]], "id": 18446744073709551615, "min": -9223372036854775808'
  for (const source of [
    `{ "body": null, ${remaining} }`,
    `{ ${remaining}, "body": null }`,
    `{ "body": {}, ${remaining}, "bo\\u0064y": null, "body": false }`,
  ]) {
    const result = withoutRequestBody(source)
    expect(result).toContain(remaining)
    expect(hasRequestBody(result)).toBe(false)
    expect(() => JSON.parse(result)).not.toThrow()
  }
  expect(withoutRequestBody('{"headers":{}, "body":{}, "extra":true}')).toBe(
    '{"headers":{}, "extra":true}'
  )
  expect(withoutRequestBody('{"body": {}, "body": null}')).toBe("{}")
  expect(withoutRequestBody(' {"body": null} ')).toBe(" {} ")
  expect(withoutRequestBody('{ "headers": {} }')).toBe('{ "headers": {} }')
  expect(() => withoutRequestBody('{"body":')).toThrow()
})
