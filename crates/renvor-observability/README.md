# renvor-observability

Observability for the [Renvor](https://github.com/renvor-rs/renvor) framework: a structured JSON subscriber with central field-name redaction, a Prometheus text renderer over the kernel's bounded metrics port, liveness and readiness documents (HTTP routes behind `http`), and OpenTelemetry OTLP trace export behind `otel`. Every function returns a value for the application to install; this crate never installs a global subscriber.

**Prerelease. Nothing here is published and no API is stable.**

## Licence

`MIT OR Apache-2.0`.
