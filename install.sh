#!/bin/sh
set -eu

REPO="SulthanZahran1/rust_mutant"
VERSION="${RUST_MUTANT_VERSION:-latest}"
INSTALL_DIR="${RUST_MUTANT_INSTALL_DIR:-$HOME/.local/bin}"

case "$(uname -s):$(uname -m)" in
  Linux:x86_64) TARGET="x86_64-unknown-linux-gnu"; EXT="tar.gz" ;;
  Darwin:x86_64) TARGET="x86_64-apple-darwin"; EXT="tar.gz" ;;
  Darwin:arm64) TARGET="aarch64-apple-darwin"; EXT="tar.gz" ;;
  *) echo "rust-mutant installer: unsupported platform $(uname -s)/$(uname -m)" >&2; exit 2 ;;
esac

if [ "$VERSION" = latest ]; then
  BASE="https://github.com/$REPO/releases/latest/download"
else
  BASE="https://github.com/$REPO/releases/download/v$VERSION"
fi
ARCHIVE="rust-mutant-$TARGET.$EXT"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

curl --fail --location --silent --show-error "$BASE/$ARCHIVE" -o "$TMP/$ARCHIVE"
curl --fail --location --silent --show-error "$BASE/SHA256SUMS" -o "$TMP/SHA256SUMS"
EXPECTED="$(awk -v name="$ARCHIVE" '$2 == name || $2 == "*" name {print $1; exit}' "$TMP/SHA256SUMS")"
[ -n "$EXPECTED" ] || { echo "rust-mutant installer: checksum entry missing for $ARCHIVE" >&2; exit 2; }
if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL="$(sha256sum "$TMP/$ARCHIVE" | awk '{print $1}')"
else
  ACTUAL="$(shasum -a 256 "$TMP/$ARCHIVE" | awk '{print $1}')"
fi
[ "$EXPECTED" = "$ACTUAL" ] || { echo "rust-mutant installer: checksum verification failed" >&2; exit 2; }

mkdir -p "$TMP/unpack" "$INSTALL_DIR"
tar -xzf "$TMP/$ARCHIVE" -C "$TMP/unpack"
BIN="$(find "$TMP/unpack" -type f -name rust-mutant -print -quit)"
[ -n "$BIN" ] || { echo "rust-mutant installer: binary missing from archive" >&2; exit 2; }
install -m 0755 "$BIN" "$INSTALL_DIR/rust-mutant"
"$INSTALL_DIR/rust-mutant" --version
printf 'installed rust-mutant in %s\n' "$INSTALL_DIR"
