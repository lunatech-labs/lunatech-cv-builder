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
