# Repository instructions

- Project context, the YouTrack plan (project `VTG`), decisions and agent-created documents are indexed in `./docs/README.md`. Start there and follow it for working rules, placement and lifecycle.
- Run in development with `pnpm install`, then `pnpm tauri dev`.
- A change is finished when `pnpm check` passes (fail-fast: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `tsc --noEmit`, ESLint, `vitest run`) and these docs describe it. Before reporting or committing, search `docs` for what the change touched and update every document it made stale, following the index's *Keeping it current*.
