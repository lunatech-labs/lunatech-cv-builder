# Fixtures

Synthetic CVs the dev server seeds when **all three** conditions hold:

1. the database is empty (no `cvs` rows yet);
2. Keycloak is **not** configured (so we're already in dev mode);
3. `DEV_SEED_FIXTURES=1` is set in the env (the Makefile sets it for `make dev`; production deployments never do).

The triple gate is intentional belt-and-braces: production has Keycloak set, doesn't run via the Makefile, and the table isn't empty anyway, so any single one of the three would already block seeding — but together they mean a misconfigured prod redeploy cannot touch the data.

Each `*.yaml` file here becomes one CV owned by the dev user (`00000000-0000-0000-0000-000000000000`) on first boot. The personas are deliberately fictional — we never want real consultant data in the public repo.

## Adding a fixture

Drop another `NN-firstname-lastname.yaml` in this directory. They're loaded in lexicographic order, so the `NN-` prefix decides which one shows up first on the overview. Schema is the standard one documented in the top-level [README](../../README.md#yaml-schema).

## Re-seeding

The seeder only runs when the table is empty. To re-seed from scratch:

```bash
make reset       # drops + recreates the cvbuilder database
cargo run        # migrations + fixture seed run on next boot
```
