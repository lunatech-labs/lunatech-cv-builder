# CV Builder — context for Claude

This file is loaded automatically when Claude Code is invoked from this directory. Read it before answering questions about the project.

## What this is

Web app that lets Lunatech recruiters edit consultant CVs as YAML, persists them in Postgres, and renders branded PDFs through the Typst Rust library. The CV format follows the 8-criteria rubric of the `cv-reviewer` Anthropic skill (role, team size, client interaction, source of pride, added value, contributions, technologies, dates).

## Stack

- **Backend**: Rust (edition 2024), `axum` 0.8, `sqlx` 0.8 with Postgres, `tokio`, `tower-http`
- **PDF**: `typst` 0.14 + `typst-pdf` + `typst-kit` (used as a library, not the CLI)
- **Frontend**: single static HTML page (`frontend/index.html`) — no framework, no build step. Uses `js-yaml` from a CDN for client-side YAML parsing
- **DB**: Postgres 16 in Docker (`docker-compose.yml`), exposed on `localhost:5433`
- **Migrations**: SQL files in `migrations/`, run by `sqlx::migrate!` at startup

## Layout

```
src/main.rs          thin entry, calls into lib
src/lib.rs           re-exports Db; exposes `api_router(db)` and `app(db, frontend_dir)`
src/db.rs            sqlx queries (list / create / get / update / delete)
src/handlers.rs      HTTP handlers; PDF route reads ?theme= query param
src/pdf.rs           YAML -> Typst dict literal -> compile -> PDF bytes
assets/cv.typ        Typst template (mirrors the HTML preview visually)
frontend/index.html  YAML editor + live HTML preview + Open/Save/PDF UI
migrations/          schema (single `cvs` table)
tests/api.rs         integration tests (use `#[sqlx::test]` for per-test DBs)
.cargo/config.toml   sets DATABASE_URL for cargo run / test
docker-compose.yml   Postgres service
```

## Run

```bash
docker-compose up -d                  # Postgres on :5433
cargo run                              # app on :3000
# Then open http://127.0.0.1:3000
```

Optional env vars:
- `DATABASE_URL` — defaults to `postgres://cvbuilder:cvbuilder@localhost:5433/cvbuilder`
- `CV_DEBUG_TYPST=1` — writes the generated Typst source to `/tmp/cv-builder-debug.typ` for each PDF render

## API

| Method | Path                       | Purpose                              |
| ------ | -------------------------- | ------------------------------------ |
| GET    | `/api/cvs`                 | list `[{id, name, updated_at}]`      |
| POST   | `/api/cvs`                 | body `{yaml}` -> `{id}`              |
| GET    | `/api/cvs/{id}`            | `{id, name, yaml, updated_at}`       |
| PUT    | `/api/cvs/{id}`            | body `{yaml}`                        |
| DELETE | `/api/cvs/{id}`            | -                                    |
| GET    | `/api/cvs/{id}/pdf?theme=` | PDF bytes; theme = cosmic/luxe/opera |

The `name` column is extracted from the YAML's `name:` key on save and used for the list view.

## Two-renderer rule

The HTML preview (`frontend/index.html`) and the Typst template (`assets/cv.typ`) are independent renderers of the same YAML schema. Visual changes must be applied to **both** to stay in sync. The HTML uses CSS + DOM, the Typst version uses `polygon()`, `grid()`, `place()` — they cannot share code, only intent.

The browser's "CSS Style" tab is local-only and does not affect the backend PDF — that is intentional.

## Typst gotchas hit during development

- **`Library::default()` / `Library::builder()`** require `use typst::LibraryExt;` — they live on the trait, not the struct.
- **Multi-line `if/else if/else` does not chain** when used as an expression. The newline between branches ends the `if`. Use a dict lookup instead:
  ```typst
  #let palettes = (cosmic: ..., luxe: ..., opera: ...)
  #let p = if theme in palettes { palettes.at(theme) } else { palettes.cosmic }
  ```
- **`rect(width: 1fr)` is invalid** — `1fr` only works in `grid()` column/row tracks. Use `width: 100%` inside a grid cell.
- **Empty mappings/arrays differ**: `()` is an empty *array*, not a dict, in Typst. The Rust serialiser emits `(key: val,)` (with trailing comma) for non-empty maps so the parse stays unambiguous.
- **Type comparisons**: prefer `type(x) == str` over `type(x) == "string"` — the latter sometimes silently fails.

## sqlx specifics

- We use **runtime queries** (`sqlx::query(...)` + `.bind(...)`) and not the `query!` macro. The macro requires `DATABASE_URL` at compile time, which adds friction without payoff at this scale.
- Integration tests use `#[sqlx::test]`, which creates a fresh scratch database (`_sqlx_test_<n>`) on the cluster pointed at by `DATABASE_URL`, runs migrations, and tears down after the test. The `DATABASE_URL` env var must point at a cluster where the user can `CREATE DATABASE` (the docker-compose Postgres user is a superuser, so this just works).

## Tests

```bash
docker-compose up -d
cargo test                       # all 48 tests
cargo test --test api            # integration tests only
cargo test --lib pdf             # pdf unit tests only
```

When adding a feature, expect to update three places: `handlers.rs` (the route), `tests/api.rs` (an integration test), and one of the unit test modules if there's testable logic in isolation. Don't merge a feature without test coverage — the existing matrix should not regress.

## Fonts

The Typst template requests Poppins/Inter to match the HTML look. They are not bundled — Typst falls back to whatever the system has, so the PDF font is close but not identical to the browser preview. To get an exact match, drop `.ttf` files into `assets/fonts/` and load them via `typst_kit::fonts::FontSearcher::with_search_path` (not yet wired up).

## Things that should NOT regress

- The `cv-reviewer` 8 criteria must remain expressible in the YAML schema (each `experiences[]` entry has fields for role, team size, client interaction, source of pride, added value, contributions, technologies, dates — though the skeleton schema is loose so users can add fields as they go).
- The frontend remains a single static HTML page, no build tooling.
- PDF generation stays in-process (`typst` crate), not a CLI subprocess.
