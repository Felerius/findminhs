#!/usr/bin/env bash

RUST_VERSION=1.52.1
RUST_TARGET=x86_64-unknown-linux-gnu

SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"
REPO_DIR="$(dirname "$(dirname "${SCRIPT_DIR}")")"


cargo "+${RUST_VERSION}" build \
    --manifest-path="${REPO_DIR}/Cargo.toml" \
    --target="${RUST_TARGET}" \
    --release

cp "${REPO_DIR}/target/${RUST_TARGET}/release/findminhs" "${SCRIPT_DIR}"
