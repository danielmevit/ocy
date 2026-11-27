#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

VERSION="$(python3 - <<'PY'
import tomllib

with open("rust/Cargo.toml", "rb") as source:
    print(tomllib.load(source)["workspace"]["package"]["version"])
PY
)"

export MACOSX_DEPLOYMENT_TARGET="11.0"

cd rust
rustup target add aarch64-apple-darwin x86_64-apple-darwin

# Remap build-machine paths out of the binary (CR2c). Every FROM is non-empty:
# an empty FROM makes rustc prefix every *relative* path with the TO, and rustc
# applies the LAST matching rule, so the most specific prefixes come last. The
# sysroot is queried inside rust/ so rustup resolves rust-toolchain.toml's pinned
# toolchain, not the runner's default.
sysroot="$(rustc --print sysroot)"
sep=$'\037'
flags=""
add_remap() {
    [ -n "$1" ] || return 0
    if [ -n "$flags" ]; then flags+="$sep"; fi
    flags+="--remap-path-prefix=$1=$2"
}
add_remap "${RUSTUP_HOME:-$HOME/.rustup}" rustup
add_remap "$ROOT" ocy
add_remap "${CARGO_HOME:-$HOME/.cargo}" cargo
add_remap "${CARGO_TARGET_DIR:-$ROOT/rust/target}" target
add_remap "$sysroot" rustup
export CARGO_ENCODED_RUSTFLAGS="$flags"
echo "CARGO_ENCODED_RUSTFLAGS: $(printf '%s' "$flags" | tr '\037' ' ')"

cargo build --release --locked --target aarch64-apple-darwin -p ocy-app
cargo build --release --locked --target x86_64-apple-darwin -p ocy-app
cd "$ROOT"

ARM64_BIN="rust/target/aarch64-apple-darwin/release/Ocy"
X86_BIN="rust/target/x86_64-apple-darwin/release/Ocy"

APP="packaging/macos/dist/Ocy.app"
DIST="packaging/macos/dist"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

lipo -create "$ARM64_BIN" "$X86_BIN" -output "$APP/Contents/MacOS/Ocy"
chmod 0755 "$APP/Contents/MacOS/Ocy"

ICONSET="$DIST/Ocy.iconset"
rm -rf "$ICONSET"
mkdir -p "$ICONSET"
SOURCE_ICON="assets/icons/app/png/ocy-app-256.png"
VECTOR_ICON="assets/icons/app/ocy-app.svg"
TMP_ICON_DIR="$(mktemp -d "${TMPDIR:-/tmp}/ocy-icon.XXXXXX")"

if qlmanage -t -s 1024 -o "$TMP_ICON_DIR" "$VECTOR_ICON" >/dev/null 2>&1 &&
    test -f "$TMP_ICON_DIR/ocy-app.svg.png"; then
    LARGE_ICON="$TMP_ICON_DIR/ocy-app.svg.png"
    echo "Icon source: vector $VECTOR_ICON rendered to 1024 px"
    for SIZE in 16 32 128 256 512; do
        sips -z "$SIZE" "$SIZE" "$LARGE_ICON" --out "$ICONSET/icon_${SIZE}x${SIZE}.png" >/dev/null
    done
    for SIZE in 16 32 128 256 512; do
        DOUBLE=$((SIZE * 2))
        sips -z "$DOUBLE" "$DOUBLE" "$LARGE_ICON" --out "$ICONSET/icon_${SIZE}x${SIZE}@2x.png" >/dev/null
    done
else
    echo "Icon source: fallback 256 px PNG $SOURCE_ICON"
    for SIZE in 16 32 128 256; do
        sips -z "$SIZE" "$SIZE" "$SOURCE_ICON" --out "$ICONSET/icon_${SIZE}x${SIZE}.png" >/dev/null
    done
    for SIZE in 16 32 128; do
        DOUBLE=$((SIZE * 2))
        sips -z "$DOUBLE" "$DOUBLE" "$SOURCE_ICON" --out "$ICONSET/icon_${SIZE}x${SIZE}@2x.png" >/dev/null
    done
fi
rm -rf "$TMP_ICON_DIR"
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Ocy.icns"

python3 - "$VERSION" <<'PY'
import pathlib
import plistlib
import sys

version = sys.argv[1]
source = pathlib.Path("packaging/macos/Info.plist.in").read_bytes()
source = source.replace(b"@VERSION@", version.encode("ascii"))
info = plistlib.loads(source)
with open("packaging/macos/dist/Ocy.app/Contents/Info.plist", "wb") as output:
    plistlib.dump(info, output, sort_keys=False)
PY

codesign --force --sign - "$APP"

ZIP="packaging/macos/dist/Ocy-${VERSION}-macos-universal.zip"
rm -f "$ZIP" "$ZIP.sha256"
(
    cd "$DIST"
    ditto -c -k --keepParent "Ocy.app" "$(basename "$ZIP")"
)
shasum -a 256 "$ZIP" | awk '{print $1 "  " $2}' > "$ZIP.sha256"

printf 'Ocy macOS bundle: %s\n' "$APP"
printf 'Archive: %s\n' "$ZIP"
printf 'Checksum: %s\n' "$ZIP.sha256"
codesign -dv "$APP" 2>&1
lipo -info "$APP/Contents/MacOS/Ocy"
