# Roadmap

Features queued for the CV Builder, not yet started.

## ~~Google authentication~~ → Keycloak SSO — shipped

Originally scoped as Google OAuth, replaced with Keycloak (Lunatech already
runs one). When `KEYCLOAK_URL` / `_REALM` / `_CLIENT_ID` are set, the backend
validates Bearer JWTs against the realm's JWKS on every `/api/*` call and the
frontend redirects unauthenticated users via keycloak-js + PKCE. Without
those vars the app falls back to dev mode (no auth) so contributors don't
need a Keycloak account to work locally.

## Overview / statistics page

A dashboard surfacing aggregate information across the stored CVs:
counts, recent activity, breakdowns by role / skill / client. Exact
metrics still to define.

## ~~`cv-reviewer` skill integration~~ — shipped

`POST /api/cvs/{id}/review` calls Claude with [`assets/skills/cv-reviewer/SKILL.md`](assets/skills/cv-reviewer/SKILL.md)
as the system prompt and persists a structured review on the CV record
(`overall_score`, `verdict`, `language`, `report_markdown`, `improved_yaml`).
The frontend has a "Review with Claude" button that opens a modal with
the markdown report rendered and an "Apply improved YAML" action.

## UI redesign

The frontend has grown organically from a single static HTML file. It does
the job, but density, navigation and polish all need a pass before we
hand it to a wider audience: the overview view crams stats, top CVs and
the full catalog into one scroll; the editor's read-only banner mode and
admin affordances are bolted on; the review and batch-review modals share
CSS classes by accident rather than by design; mobile is unhandled.
Scope to define — likely a small component framework (or a build-step
HTML preprocessor) and a refreshed visual language, but the constraint
that the frontend stays trivially deployable next to the Rust binary
should hold.

## Stale-CV score decay

A CV that scored 90 a year ago and hasn't been touched since shouldn't
keep ranking next to one that was reviewed last week. Add a recency
penalty so the displayed score on the overview decays as a CV ages past
its last edit (or last review): the raw `overall_score` stays in the
`reviews` table for provenance, but the ranking and the "client ready"
tile use a decayed value. Curve and grace period TBD — a likely starting
shape is "no decay for the first 6 months, then -1 point per month, capped
at -20 points". Should also surface a hint in the editor ("last reviewed
8 months ago") so consultants know when their CV is at risk of slipping.

## MCP server — connect the CV builder to Claude

Expose the CV builder over the [Model Context Protocol](https://modelcontextprotocol.io)
so Claude (Claude Code, Claude.ai, the API) can read and edit CVs
directly without going through the HTTP API. Likely tool surface: `list_cvs`,
`get_cv`, `update_cv`, `review_cv`, `render_pdf`, plus a resource
endpoint for the cv-reviewer skill. Auth re-uses the existing Keycloak
JWT (the MCP client passes a Bearer token in the transport headers) so
ownership and admin gates continue to apply unchanged. The point is to
turn "ask Claude to tighten the description on my Disney mission" into
a one-shot instead of a copy-paste round trip through the editor.
