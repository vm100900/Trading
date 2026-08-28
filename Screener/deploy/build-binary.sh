#!/usr/bin/env bash
# Rebuild deploy/bin/screener-service — a fully static x86_64 musl binary that
# runs on any x86_64 Linux (no glibc dependency). Run this on a machine with a
# decent amount of RAM, commit the result, and the VM just pulls it: the Docker
# image build (deploy/Dockerfile) then takes seconds instead of ~an hour.
#
# Prereqs (one time):
#   rustup target add x86_64-unknown-linux-musl
#   sudo apt-get install -y musl-tools        # provides musl-gcc for `ring`
set -euo pipefail
cd "$(dirname "$0")/.."

CC_x86_64_unknown_linux_musl=musl-gcc \
  cargo build --release -p screener-service --target x86_64-unknown-linux-musl

BIN=target/x86_64-unknown-linux-musl/release/screener-service
strip "$BIN"
mkdir -p deploy/bin
cp "$BIN" deploy/bin/screener-service
chmod +x deploy/bin/screener-service

file deploy/bin/screener-service
ls -la deploy/bin/screener-service
echo "OK — commit deploy/bin/screener-service, then on the VM: docker compose up -d --build"
