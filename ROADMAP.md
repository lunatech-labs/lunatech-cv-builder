# Roadmap

Features queued for the CV Builder, not yet started.

## Google authentication

Gate the app behind Google OAuth so only Lunatech recruiters can read or
edit CVs. Today the API is fully open and the binary listens on `0.0.0.0`,
so this is the next protective layer before the app is exposed beyond a
single workstation.

## Overview / statistics page

A dashboard surfacing aggregate information across the stored CVs:
counts, recent activity, breakdowns by role / skill / client. Exact
metrics still to define.

## `cv-reviewer` skill integration

Surface the 8-criteria review (role, team size, client interaction,
source of pride, added value, contributions, technologies, dates) from
inside the app — for example a "Review with Claude" action that calls
the Anthropic API with the `cv-reviewer` skill and renders the
structured feedback. The YAML schema is already aligned with the rubric
(see `CLAUDE.md`, "Things that should NOT regress"), so the remaining
work is the call wiring and the UX for displaying / persisting reviews.
