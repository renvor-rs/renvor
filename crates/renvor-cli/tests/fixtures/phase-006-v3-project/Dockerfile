# Container development controls for legacy-v3.
FROM docker.io/library/rust:1-slim AS build
WORKDIR /src
COPY . .
RUN cargo build --release

FROM gcr.io/distroless/cc-debian12
COPY --from=build /src/target/release/legacy-v3 /app
USER nonroot:nonroot
ENTRYPOINT ["/app"]
