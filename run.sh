#!/usr/bin/env bash
# Runs phoenix-pnp with its sample. Extra arguments replace the sample: ./run.sh [ARGUMENTS]
set -u
cd "$(dirname "$0")"
pause() { [ -n "${NOPAUSE:-}" ] || { read -r -p "Press Enter to close..." _ || true; }; }
trap pause EXIT
if [ "$#" -eq 0 ]; then set -- plan examples/board.pos examples/feeders.csv examples/plan.json --nozzles 2; fi
cargo run --release --quiet -- "$@"
