# udp-director

udp-director is intended to be a lightweight, high-performance UDP traffic director. It sits between UDP clients and one or more upstream backends, and forwards packets based on configurable routing rules. Typical use‑cases include sharding UDP traffic, simple load balancing, protocol fan‑out, and acting as a UDP aware edge in front of services that speak custom UDP protocols.

Key ideas:
- Minimal latency and overhead for high packet rates
- Safe, modern Rust implementation
- Clear separation of core packet forwarding, routing rules, and observability

Note: This README provides a high‑level overview and a table of contents for deeper documentation kept in the `Docs/` directory.

## Features (planned/typical)
- Configurable routing (by source, destination, port, or custom matchers)
- Health checks and backend disable/enable
- Metrics and basic observability
- Graceful reloads of configuration
- Async I/O using Tokio (or equivalent runtime)

Your local repository may not implement all features listed above yet. Please consult the Docs for the authoritative state and design.

## Getting Started
- Rust toolchain: Rust 2024 Edition, stable channel via `rustup`
- Format: `cargo fmt`
- Lint: `cargo clippy -- -D warnings`

See the documentation below for installation, configuration, and operations guidance when available.

## Documentation (Table of Contents)
The `Docs/` directory is the primary source of truth for documentation. Start here:

- Coding Guidelines: [Docs/CodingGuidelines.md](Docs/CodingGuidelines.md)

When new documents are added under `Docs/`, please add them to this list in the same pull request.

Suggested future documents (create as needed):
- Architecture: `Docs/Architecture.md`
- Setup & Quickstart: `Docs/Setup.md`
- Operations: `Docs/Operations.md`
- Contributing: `Docs/Contributing.md`
- Release Notes: `Docs/ReleaseNotes.md`

## Contributing
Please follow the coding standards and linting rules described in the Coding Guidelines. When you add a user‑facing feature, update or add documents in `Docs/` and keep this README’s table of contents in sync.

## License
Add the project’s license information here if applicable.
