#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <version> [outdir]" >&2
  exit 2
fi

VERSION="$1"
OUTDIR="${2:-dist}"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SPEC_SRC="$ROOT_DIR/packaging/rpm/bankero.spec"

BIN="$ROOT_DIR/target/release/bankero"
if [[ ! -x "$BIN" ]]; then
  echo "Missing release binary at: $BIN" >&2
  echo "Run: cargo build --release" >&2
  exit 1
fi

mkdir -p "$OUTDIR"

TOPDIR="$(mktemp -d)"
trap 'rm -rf "$TOPDIR"' EXIT

mkdir -p "$TOPDIR/BUILD" "$TOPDIR/RPMS" "$TOPDIR/SOURCES" "$TOPDIR/SPECS" "$TOPDIR/SRPMS"

PKG_DIR="bankero-${VERSION}"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

mkdir -p "$STAGE/$PKG_DIR"
cp -f "$BIN" "$STAGE/$PKG_DIR/bankero"
cp -f "$ROOT_DIR/README.md" "$STAGE/$PKG_DIR/README.md"
cp -f "$ROOT_DIR/PRD.md" "$STAGE/$PKG_DIR/PRD.md"

TARBALL="$TOPDIR/SOURCES/bankero-${VERSION}.tar.gz"
(
  cd "$STAGE"
  tar -czf "$TARBALL" "$PKG_DIR"
)

cp -f "$SPEC_SRC" "$TOPDIR/SPECS/bankero.spec"

rpmbuild -bb \
  --define "_topdir $TOPDIR" \
  --define "version $VERSION" \
  "$TOPDIR/SPECS/bankero.spec"

RPM_PATH="$(find "$TOPDIR/RPMS" -type f -name "*.rpm" | head -n 1)"
if [[ -z "${RPM_PATH:-}" ]]; then
  echo "RPM build failed: no RPM produced" >&2
  exit 1
fi

cp -f "$RPM_PATH" "$OUTDIR/"
echo "Wrote: $OUTDIR/$(basename "$RPM_PATH")"
