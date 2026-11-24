# Swagger Petstore

This sample bundles all 20 operations from the [Swagger Petstore specification](https://petstore.swagger.io/v2/swagger.json).
The Swagger 2.0 source was converted to OpenAPI 3.0.3 for Simile. Operations run
locally in the browser; the server URLs in the contract do not send requests to
the public Petstore.

## Try a baseline

1. Select **POST /store/order** in Run.
2. Load **requests/place-order.json**, or select **Synthesize request**.
3. Select **Run baseline** to see the generated response and execution report.

Try **GET /user/{username}** next. Its baseline request needs no body, so use an
empty request document (`{}`). Baseline runs generate fresh responses independently.
Most Petstore response properties are optional, so a generated response can be `{}`.

## Try HTTP

1. Open **HTTP session** and select **Start service**.
2. Select **GET /user/{username}**, then enter `doggie` in **path: username**.
3. Select **Send request**. The local service returns a generated user response.

The contract retains its authentication declarations and media types. Secured
routes need credentials; JSON operations are the easiest starting point. Review
**Audit** for supported behavior and fidelity findings. This sample has no HCL
storage effects, so a POST does not make a later GET return that same record.
Use the Orders API sample to explore persistence, profiles, and async work.

## Source

Swagger Petstore 1.0.7, retrieved 2026-11-24 from
https://petstore.swagger.io/v2/swagger.json. Converted with swagger2openapi 7.0.8;
no operations were removed or manually rewritten. The source declares the
Apache 2.0 license, included as LICENSE.txt.
