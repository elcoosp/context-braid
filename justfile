set shell := ["zsh", "-cu"]

test: test-rust test-browser test-unit

test-rust:
    cargo nextest run --workspace

test-browser:
    pnpm --filter @context-braid/extension test:browser

test-unit:
    pnpm --filter @context-braid/extension test:unit

test-e2e: build
    pnpm --filter @context-braid/extension test:e2e

sync-check:
    pnpm run check:sync

build: sync-check
    cargo build --workspace
    pnpm --filter @context-braid/extension build
    pnpm --filter @context-braid/extension build:firefox

screenshots:
    ./scripts/capture-store-screenshots.sh

icons:
    ./scripts/sync-icons.sh
