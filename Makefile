.PHONY: dev test setup reset db-up db-down screenshots help

# Default target — show the inventory.
help:
	@echo "Targets:"
	@echo "  make setup       — first-time bootstrap (Postgres up + cargo build)"
	@echo "  make dev         — run the dev server (sources .env.local, then cargo run)"
	@echo "  make test        — cargo test (Postgres must be up) + frontend coalescer test"
	@echo "  make db-up       — bring up the Postgres container"
	@echo "  make db-down     — stop the Postgres container (data is preserved)"
	@echo "  make reset       — drop the cvbuilder DB and recreate it (re-seeds fixtures on next run)"
	@echo "  make screenshots — capture the four README screenshots via headless Chrome"

# First-time bootstrap on a fresh clone: Postgres up, wait healthy, then build.
# After this, `make dev` boots the app on :3000 and seeds fixture CVs into the
# empty database (dev mode only — see assets/fixtures/README.md).
setup: db-up
	@cargo build

db-up:
	@docker start cv-builder-pg >/dev/null 2>&1 || docker-compose up -d
	@printf "Waiting for Postgres … "
	@until docker exec cv-builder-pg pg_isready -U cvbuilder -d cvbuilder >/dev/null 2>&1; do sleep 0.5; done
	@echo "ready"

db-down:
	@docker-compose stop

# Drops + recreates the cvbuilder database. The next `make dev` (or
# `cargo run`) re-runs the migrations and re-seeds fixture CVs.
reset: db-up
	@docker exec -e PGPASSWORD=cvbuilder cv-builder-pg psql -U cvbuilder -d postgres -c "DROP DATABASE IF EXISTS cvbuilder WITH (FORCE)"
	@docker exec -e PGPASSWORD=cvbuilder cv-builder-pg psql -U cvbuilder -d postgres -c "CREATE DATABASE cvbuilder OWNER cvbuilder"
	@echo "Database cvbuilder dropped + recreated. Run 'make dev' to migrate and seed."

# Run the dev server with .env.local sourced (gitignored, holds ANTHROPIC_API_KEY
# and the KEYCLOAK_* vars when you have them). Without .env.local the server
# starts in dev mode (no auth, no Anthropic) and seeds fixture CVs on first boot.
#
# `DEV_SEED_FIXTURES=1` is set explicitly here so production deployments —
# which do NOT use this Makefile — can never accidentally trigger the
# seeder, even if their Keycloak env vars went missing.
dev:
	@if [ -f .env.local ]; then set -a; . ./.env.local; set +a; fi; DEV_SEED_FIXTURES=1 cargo run

test:
	cargo test
	node --test frontend/sync-coalescer.test.js

# Capture the four README screenshots via the headless-Chrome script in
# scripts/screenshots.mjs. Expects the dev server to be running on :3000
# (start it with 'make dev' in another terminal first) and Node.js to be
# installed.
screenshots:
	@command -v node >/dev/null || (echo "Node.js required — install via brew install node" && exit 1)
	@curl -s -o /dev/null -w "" http://127.0.0.1:3000/ 2>/dev/null || (echo "Dev server not reachable on :3000 — start it with 'make dev' first" && exit 1)
	@cd scripts && (test -d node_modules || npm install --silent puppeteer-core) && node screenshots.mjs
