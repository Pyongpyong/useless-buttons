#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

# Vercel's Node build image does not guarantee a Rust toolchain.
if [ -f "${CARGO_HOME:-$HOME/.cargo}/env" ]; then
  source "${CARGO_HOME:-$HOME/.cargo}/env"
fi
if ! command -v rustup >/dev/null 2>&1; then
  installer=$(mktemp)
  curl --proto '=https' --tlsv1.2 --fail --silent --show-error \
    https://sh.rustup.rs -o "$installer"
  sh "$installer" -y --profile minimal --default-toolchain stable
  rm -f "$installer"
  source "${CARGO_HOME:-$HOME/.cargo}/env"
fi

rustup toolchain install stable --profile minimal
rustup default stable
rustup target add wasm32-unknown-unknown

if ! command -v wasm-pack >/dev/null 2>&1 || [ "$(wasm-pack --version)" != "wasm-pack 0.15.0" ]; then
  cargo install wasm-pack --version 0.15.0 --locked
fi

npm test
npm run build:demo
