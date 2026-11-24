# Petstore source snapshot

`swagger.json` is the original response retrieved on 2026-11-24 from
https://petstore.swagger.io/v2/swagger.json (Swagger Petstore 1.0.7).

SHA-256: `bb116730d391e330119c691f9b124d07b9afdea61b5a252345b577e9a4c2f98b`.

`openapi.json` is generated from that snapshot, preserving all 20 operations,
schemas, security declarations, and server base paths. Simile accepts OpenAPI
3.x; this conversion is bundled, with no converter dependency or remote fetch
in the playground runtime. Regenerate from the repository root:

```sh
pnpm --dir playground dlx swagger2openapi@7.0.8 \
  src/templates/petstore/swagger.json \
  --outfile src/templates/petstore/openapi.json \
  --targetVersion 3.0.3 --indent '  '
```

The original `info.license` declares Apache 2.0. The accompanying LICENSE.txt
reproduces that license; README.md records the conversion for sample users.
