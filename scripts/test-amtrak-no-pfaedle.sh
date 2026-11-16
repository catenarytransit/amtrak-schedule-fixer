#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# The fixer downloads the official Amtrak GTFS ZIP, extracts it, and applies
# schedule corrections. Neither OSM nor pfaedle is required.
cargo run --release

# Run both the pure GTFS-time unit check and the live-feed LA station check.
cargo test --test amtrak_regression -- --include-ignored --nocapture

# Package the verified feed for inspection or use by other workflows.
if command -v zip >/dev/null 2>&1; then
  (cd amtrak-gtfs && zip -qr ../amtrak-gtfs-tested-no-pfaedle.zip .)
fi
