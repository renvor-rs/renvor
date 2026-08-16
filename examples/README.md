# Renvor examples

Every example here **compiles**, **runs**, and uses **no hidden global mutable state** — no
ambient singleton, no lazily-initialised global registry, no process-wide mutable default
(spec FR-032). An example that needs a global to work is demonstrating a design the framework
does not have.

No example requires a transport, a port, or a database (SC-014).
