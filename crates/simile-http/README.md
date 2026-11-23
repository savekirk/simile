# Shared HTTP handling

`handle_request` accepts an HTTP request with collected bytes and returns an
exchange: response bytes and repeated headers, matched operation, profile, seed,
fault transport plan, and sanitized incident evidence. The native listener collects
the body, logs the exchange, and enacts delays or disconnects. Browser hosts can
display those transport outcomes without opening a socket.

`HttpOptions` accepts an explicit response seed and body limit. The same seed also
controls service-declared error synthesis. HEAD responses always have an empty
body, including fallback errors. Authentication and parameter validation retain
their precedence over body-limit errors.

Webhook preparation serializes and signs bytes using a supplied timestamp. Work
ordering, readiness, and retry delays are shared; hosts own delivery and scheduling.
