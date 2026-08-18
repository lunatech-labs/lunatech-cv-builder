# CV Builder — context for Claude

This file is loaded automatically when Claude Code is invoked from this directory. Read it before answering questions about the project.

## What this is

Web app that lets Lunatech recruiters edit consultant CVs as YAML, persists them in Postgres, and renders branded PDFs through the Typst Rust library. The CV format follows the 8-criteria rubric of the `cv-reviewer` Anthropic skill (role, team size, client interaction, source of pride, added value, contributions, technologies, dates).

## Stack

- **Backend**: Rust (edition 2024), `axum` 0.8, `sqlx` 0.8 with Postgres, `tokio`, `tower-http`
- **PDF**: `typst` 0.14 + `typst-pdf` + `typst-kit` (used as a library, not the CLI)
- **Frontend**: single static HTML page (`frontend/index.html`) — no framework, no build step. Uses `js-yaml` from a CDN for client-side YAML parsing
- **DB**: Postgres 18 in Docker (`docker-compose.yml`), exposed on `localhost:5433`
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
migrations/          schema (`cvs` and `users` tables)
tests/api.rs         integration tests (use `#[sqlx::test]` for per-test DBs)
.cargo/config.toml   sets DATABASE_URL for cargo run / test
docker-compose.yml   Postgres service
```

## Run

```bash
make setup    # docker-compose up + cargo build (one-time)
make dev      # boots app on :3000, sources .env.local if present
# Then open http://127.0.0.1:3000
```

`make dev` sets `DEV_SEED_FIXTURES=1`, which the binary reads to decide whether to seed the synthetic CVs under `assets/fixtures/` into an empty database. The seeder triple-gates on (Keycloak missing) ∧ (DB empty) ∧ (`DEV_SEED_FIXTURES=1`); production deployments don't run via the Makefile so they never set the var, which is the load-bearing safeguard against an accidental prod seed if a Keycloak env var ever disappeared.

Other targets in the Makefile: `make test` (cargo test, Postgres must be up), `make reset` (drop + recreate the cvbuilder DB → next `make dev` re-seeds), `make db-up` / `make db-down`, `make screenshots` (regenerate the README screenshots — see [scripts/screenshots.mjs](scripts/screenshots.mjs)).

Optional env vars:
- `DATABASE_URL` — defaults to `postgres://cvbuilder:cvbuilder@localhost:5433/cvbuilder`
- `BIND_ADDR` — defaults to `0.0.0.0:3000` (e.g. `BIND_ADDR=127.0.0.1:8080`)
- `ANTHROPIC_API_KEY` — enables `POST /api/review`. Without it, the route returns 503 and the rest of the API is unaffected.
- `KEYCLOAK_URL`, `KEYCLOAK_REALM`, `KEYCLOAK_CLIENT_ID` — together gate `/api/*` (except `/api/config`) behind a Bearer JWT validated against Keycloak. When **any** of the three is missing the app runs unauthenticated (dev mode, with a warning log) so contributors without Keycloak access can still work.
- `ADMIN_EMAILS` — comma-separated allow-list (case-insensitive) of Keycloak emails who get the admin flag and can write to any CV. Empty / missing = no admins.
- `DEV_SEED_FIXTURES=1` — only set by the Makefile's `dev` target. Required for the empty-DB fixture seeding to fire — see above.
- `CV_DEBUG_TYPST=1` — writes the generated Typst source to `/tmp/cv-builder-debug.typ` for each PDF render

## API

| Method | Path                       | Purpose                                                                | Scope    |
| ------ | -------------------------- | ---------------------------------------------------------------------- | -------- |
| GET    | `/api/health`              | `{status, database, detail?}` — 200 when ready, 503 otherwise           | public   |
| GET    | `/api/overview`            | `{me, stats, my_cvs, top_cvs}` — landing-page payload                  | self     |
| GET    | `/api/cvs`                 | list `[{id, name, updated_at}]` of the caller's CVs                    | self     |
| POST   | `/api/cvs`                 | body `{yaml}` -> `{id}` (assigned to caller)                           | self     |
| GET    | `/api/cvs/{id}`            | `{id, name, yaml, updated_at, owner, latest_review?, latest_review_at?}` | any user |
| PUT    | `/api/cvs/{id}`            | body `{yaml}` — owner only                                             | owner    |
| DELETE | `/api/cvs/{id}`            | — owner only                                                           | owner    |
| GET    | `/api/cvs/{id}/pdf?theme=` | PDF bytes; theme = lunatech/cosmic/luxe/opera                          | any user |
| POST   | `/api/cvs/{id}/reviews`    | runs Claude on the saved YAML, persists, returns the review            | owner    |
| POST   | `/api/review/pdf`          | body `{review, cv_name?}` -> PDF bytes (download); stateless           | any user |

## Startup and health

The process **binds its port before touching Postgres**. `Db::lazy` builds the pool without dialling, `main` binds, and migrations plus the seniority backfill then run in a background task that retries with exponential backoff (1s, capped at 30s) until they succeed.

This ordering is load-bearing. Connecting eagerly meant an unreachable database stalled startup before the listener existed, so the platform proxy saw a dead port and returned 504 on *every* route — the static frontend and the public `/api/config` included — with nothing logged until the process finally gave up. Now an unreachable database is a degraded app, not an invisible outage: the frontend still serves, the failure is logged on every retry with the root cause, and a database that comes back is picked up without a restart.

`GET /api/health` reports the current state. It sits on the public router, alongside `/api/config` and outside the JWT layer, because a check requiring a valid token cannot distinguish "app down" from "Keycloak down".

| `status`   | Code | Meaning                                                  |
| ---------- | ---- | -------------------------------------------------------- |
| `ok`       | 200  | Migrations applied and the database answers a live ping. |
| `starting` | 503  | Serving, migrations still running. Expected briefly on every boot. |
| `degraded` | 503  | Database unreachable; `detail` carries the reason.       |

The ping is a live round-trip, not a cached boot-time verdict, so a database that disappears after a healthy boot still reports honestly. It is capped at 3s (`PING_TIMEOUT` in [`src/db.rs`](src/db.rs)) — the pool's own acquire timeout is 30s, which would make health hang past a gateway timeout and 504 like everything else, defeating its purpose.

**Scope semantics** — `self` means the response is filtered to the caller's CVs. `any user` means any authenticated user can read (Lunatech-internal trust model: every recruiter can browse every consultant's CV). `owner` means only the CV's creator can mutate. The `owner` field on `GET /api/cvs/{id}` lets the frontend detect non-owned CVs and switch the editor into a read-only banner mode.

The `name` column is extracted from the YAML's `name:` key on save and used for the list view.

## Data model

Three tables, all in [`migrations/`](migrations/):

- `users` — keyed on the Keycloak `sub` claim (UUID). Holds `email`, `name`, `created_at`, `last_seen_at`. Auto-populated by the auth middleware on first login (upsert).
- `cvs` — `user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE`. Every read / write is scoped on `user_id` so a CV from another user is invisible (looks like 404). When a user is deleted the cascade removes their CVs.
- `reviews` — every Claude review is persisted with `cv_id` (cascade), `user_id` (denormalised cascade), `overall_score` / `verdict` / `language` (denormalised top-level columns for cheap aggregation), `payload JSONB` (full review object), `yaml_snapshot TEXT` (the YAML that was actually reviewed), and `created_at`. Indexes on `(cv_id, created_at DESC)` and `(user_id, created_at DESC)` for the upcoming history / overview surfaces.

`cvs` also carries three cached seniority columns (`seniority JSONB`, `seniority_score SMALLINT`, `seniority_level TEXT`) — see Seniority below.

In dev mode (no Keycloak configured) all requests resolve to a fixed **dev user** with id `00000000-0000-0000-0000-000000000000` — seeded by migration `0004_users.sql` and used as the owner for any pre-existing CVs that didn't have a `user_id` yet. Keeps the local hack-on-it-without-auth flow working unchanged.

The user resolution lives in [`src/users.rs`](src/users.rs) as a Tower middleware that always runs on `/api/cvs/*` and `/api/review*`. It reads the `Claims` extension that the auth layer set (when present), parses the `sub` as a UUID, upserts the row, and stashes the `User` as a request extension. Handlers extract via `Extension<User>` and pass `user.id` into the `db.rs` calls.

**Admin role.** The `ADMIN_EMAILS` env var (comma-separated, case-insensitive) controls who gets the `is_admin: true` flag on their resolved `User`. Admins can update, delete, and trigger reviews on any CV regardless of ownership — the write handlers (`update_cv` / `delete_cv` / `review_cv`) all run a `require_write_access` check that succeeds when `user.is_admin` *or* `cv.owner.id == user.id`, and dispatch to the unscoped `db.update_any` / `db.delete_any` variants. Reviews persisted by an admin are still attributed to the admin's `user_id`, not the owner's, so provenance stays accurate. The dev user is intentionally not admin — any privilege escalation has to go through Keycloak.

## Overview / landing page

`GET /api/overview` is the single round-trip the post-login landing page makes. It returns:
- `me` — the resolved `User` (id, email, name)
- `stats` — split into `mine` (caller-scoped) and `company` (workspace-wide), each carrying `total_cvs`, `reviewed_cvs`, `avg_score` (over latest reviews), `client_ready_count`. One SQL with shared CTE so both sides return in a single round-trip.
- `my_cvs` — every CV owned by the caller, with the score / verdict / timestamp of its latest review (denormalised columns from the `reviews` table via a `LEFT JOIN LATERAL`)
- `top_cvs` — top 10 across the platform, ranked by `latest_score DESC` then recency, with `owner_name` so each row credits the consultant

Everyone in the realm can see everyone's reviewed CVs in the ranking — that's the point. Edit / delete / re-review remain owner-only. The frontend lives in a single `index.html` with two views toggled by query string: `/` → overview, `/?id=xxx` → editor (read-only when `cv.owner.id !== me.id`), `/?new=1` → blank editor.

## Editor save and the unsaved-changes guard

The editor has a dedicated **Save** button (`onSaveClick()`, calling `saveCv()` directly), plus three actions that save implicitly as a side effect: **Review** (the server-side review endpoint reads the stored YAML with no request body, so a row has to exist first), and **Preview PDF** / **Export PDF** (so the rendered PDF matches what is on screen and the URL stays stable across a refresh). There is no autosave on typing.

Leaving the editor while dirty (the back arrow, the sidebar brand/logo, or picking a different CV from the sidebar list) all funnel through `navigateTo()`, which checks `hasUnsavedChanges()` and, if there are unsaved edits, awaits a small custom modal (`#unsaved-modal-bg`, driven by `promptUnsavedChanges()` / `resolveUnsavedPrompt()`) offering **Save & leave**, **Discard**, or **Cancel**. `deleteCv()` opts out via `navigateTo(url, {skipGuard: true})` since it already ran its own delete confirmation and the in-editor content no longer corresponds to anything worth keeping. Tab close/refresh is covered separately by a `beforeunload` handler using the browser's native prompt, since custom buttons aren't allowed there.

## Seniority

[`src/seniority.rs`](src/seniority.rs) is a Rust port of `seniority_score.py` — a transparent 0-100 grade derived from the YAML CV. Five dimensions add up to 100: years of experience (30), leadership signals (25), scope of contributions (20), external signals (15), title bonus (10). Total is bucketed into Junior / Mid-level / Senior / Staff / Tech Lead / Principal. The grid lives in constants at the top of the module — tune them to match Lunatech's house calibration.

Computed on every `db.create` / `db.update` and persisted into three columns on `cvs`: `seniority JSONB` (the full per-dimension breakdown for the editor's tooltip), `seniority_score SMALLINT` and `seniority_level TEXT` (denormalised for cheap dashboard queries). `Db::connect` runs a one-shot `backfill_seniority` after migrations so any pre-existing CV gets scored on first boot.

The frontend surfaces it as a colour-coded chip (grey → blue → purple → gold → red, by level) in every rank row, every "My CVs" card, and the editor header (with a hover-tooltip breaking down the points per dimension).

## Authentication (Keycloak via OIDC)

When the three `KEYCLOAK_*` env vars are set, the backend validates a Bearer JWT on every `/api/*` call (except `/api/config`, which is intentionally public so the frontend can bootstrap). Validation: signature against the realm's JWKS (fetched once at startup from `{KEYCLOAK_URL}/realms/{REALM}/protocol/openid-connect/certs`), `iss` pinned to the realm, `aud` pinned to `account` (Keycloak's default).

The frontend uses [keycloak-js](https://www.keycloak.org/securing-apps/javascript-adapter), loaded directly from the Keycloak server at `{KEYCLOAK_URL}/js/keycloak.min.js` (version-locked to the realm). On page load it fetches `/api/config`, then if Keycloak is configured calls `keycloak.init({onLoad: 'login-required', pkceMethod: 'S256'})`, which redirects unauthenticated users to the Keycloak login. After auth, every `fetch('/api/...')` is wrapped to attach `Authorization: Bearer ${kc.token}`, with auto-refresh.

When any `KEYCLOAK_*` var is missing the backend starts unauthenticated (warning log) and the frontend skips the redirect and Bearer header — useful for local dev without Keycloak access.

**Keycloak client setup** (one-time admin task per realm): create a public OIDC client (no secret), set Valid redirect URIs to the app's URL with `/*` suffix, set Web Origins to `+`, enable PKCE with `S256`. The token's `aud` claim must include `account` — that's the default Keycloak mapping; nothing extra to configure.

## Review with Claude

`POST /api/cvs/{id}/reviews` is the entry point. The CV must already be saved and owned by the caller — the server reads the stored YAML (no body), calls Claude (`claude-opus-4-7`, adaptive thinking + `effort: high`) with [`assets/skills/cv-reviewer/SKILL.md`](assets/skills/cv-reviewer/SKILL.md) as the system prompt, and persists the structured response in `reviews` alongside a snapshot of the YAML at review time (so a later edit on the CV doesn't silently change "what we critiqued"). Output is constrained by a JSON schema (`overall_score`, `verdict`, `language`, `report_markdown`, `improved_yaml`). Wall time is typically 20-60s; the reqwest client has a 5-min timeout, no streaming. The skill file is the single source of truth for the rubric — edit it to tune the review.

The **Review** button in the frontend implicitly saves the CV first (so review-able means save-able); the editor also has a standalone **Save** button for saving without triggering a review. On `GET /api/cvs/{id}` the server includes the most recent review under `latest_review` + `latest_review_at`, so the score badge is populated as soon as the CV loads, with no extra round-trip.

`POST /api/review/pdf` accepts `{review, cv_name?}` and returns a Lunatech-branded PDF rendering of the report. The body's `report_markdown` is converted to Typst via [`src/review_pdf.rs`](src/review_pdf.rs) (using `pulldown-cmark` for the markdown→Typst syntax mapping — headings, lists, tables, bold/italic, code), then compiled through the same shared `pdf::compile` helper as the CV. The route is stateless: nothing in the DB, no Claude call. Triggered from the "↓ Export PDF" button in the review modal once a review is in memory.

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
