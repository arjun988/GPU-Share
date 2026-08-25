#!/usr/bin/env bash
# GPUMesh installer — Phase 4
# Usage: curl -fsSL https://install.gpumesh.dev | sh
#    or: ./scripts/install.sh
set -euo pipefail

REPO="${GPUMESH_REPO:-gpumesh/gpumesh}"
BIN_DIR="${GPUMESH_BIN_DIR:-$HOME/.local/bin}"
VERSION="${GPUMESH_VERSION:-latest}"

info() { printf '→ %s\n' "$*"; }
ok() { printf '✔ %s\n' "$*"; }
err() { printf '✖ %s\n' "$*" >&2; exit 1; }

need() { command -v "$1" >/dev/null 2>&1 || err "missing dependency: $1"; }

detect_target() {
  local os arch
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m)"
  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
    *) err "unsupported arch: $arch" ;;
  esac
  case "$os" in
    linux) echo "${arch}-unknown-linux-gnu" ;;
    darwin) echo "${arch}-apple-darwin" ;;
    mingw*|msys*|cygwin*) echo "${arch}-pc-windows-msvc" ;;
    *) err "unsupported OS: $os" ;;
  esac
}

install_from_cargo() {
  need cargo
  info "Building GPUMesh from source with cargo…"
  if [[ -f Cargo.toml ]] && grep -q 'name = "gpumesh-cli"' crates/gpumesh-cli/Cargo.toml 2>/dev/null; then
    cargo install --path crates/gpumesh-cli --root "$HOME/.gpumesh-install" --force
    mkdir -p "$BIN_DIR"
    cp "$HOME/.gpumesh-install/bin/gpumesh" "$BIN_DIR/gpumesh"
  else
    cargo install --git "https://github.com/${REPO}" --locked gpumesh-cli --force --root "$HOME/.gpumesh-install"
    mkdir -p "$BIN_DIR"
    cp "$HOME/.gpumesh-install/bin/gpumesh" "$BIN_DIR/gpumesh"
  fi
  ok "Installed to $BIN_DIR/gpumesh"
}

install_from_release() {
  need curl
  local target asset url tmp
  target="$(detect_target)"
  asset="gpumesh-${VERSION}-${target}.tar.gz"
  if [[ "$VERSION" == "latest" ]]; then
    url="https://github.com/${REPO}/releases/latest/download/gpumesh-${target}.tar.gz"
  else
    url="https://github.com/${REPO}/releases/download/${VERSION}/${asset}"
  fi
  tmp="$(mktemp -d)"
  info "Downloading $url"
  if ! curl -fsSL "$url" -o "$tmp/gpumesh.tgz"; then
    info "Release asset not found — falling back to cargo install"
    install_from_cargo
    return
  fi
  tar -xzf "$tmp/gpumesh.tgz" -C "$tmp"
  mkdir -p "$BIN_DIR"
  install -m 755 "$tmp/gpumesh" "$BIN_DIR/gpumesh"
  ok "Installed to $BIN_DIR/gpumesh"
}

main() {
  echo "GPUMesh installer"
  echo "  Turn idle GPUs into a personal compute network."
  echo
  mkdir -p "$BIN_DIR"
  if [[ "${GPUMESH_FROM_SOURCE:-}" == "1" ]] || ! command -v curl >/dev/null 2>&1; then
    install_from_cargo
  else
    install_from_release
  fi
  case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *)
      info "Add to PATH:"
      echo "  export PATH=\"$BIN_DIR:\$PATH\""
      ;;
  esac
  echo
  info "Next steps:"
  echo "  gpumesh init"
  echo "  gpumesh doctor"
  echo "  gpumesh share"
  echo
  if command -v gpumesh >/dev/null 2>&1 || [[ -x "$BIN_DIR/gpumesh" ]]; then
    "$BIN_DIR/gpumesh" --version 2>/dev/null || true
  fi
  ok "Done"
}

main "$@"
