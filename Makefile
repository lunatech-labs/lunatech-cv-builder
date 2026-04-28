.PHONY: dev test

# Run the dev server with .env.local sourced (gitignored, holds ANTHROPIC_API_KEY).
dev:
	@if [ -f .env.local ]; then set -a; . ./.env.local; set +a; fi; cargo run

test:
	cargo test
