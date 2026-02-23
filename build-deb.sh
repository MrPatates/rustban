#!/usr/bin/env bash
set -euo pipefail

APP_NAME="rustban"
APP_ID="com.rustban.app"
APP_DISPLAY_NAME="RustBAN"
APP_SUMMARY="VBAN-only PipeWire control panel"
MAINTAINER="RustBAN <noreply@localhost>"

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR"
DIST_DIR="$PROJECT_DIR/dist"
BUILD_DIR="$DIST_DIR/.build"
TARGET_DIR="$PROJECT_DIR/target/release"

BINARY_PATH="$TARGET_DIR/$APP_NAME"
ICON_PATH="$PROJECT_DIR/logo.png"

log() {
    printf '[INFO] %s\n' "$*"
}

err() {
    printf '[ERR ] %s\n' "$*" >&2
}

need_cmd() {
    local cmd="$1"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        err "Missing command: $cmd"
        exit 1
    fi
}

read_version() {
    awk -F ' *= *' '/^version *=/ { gsub(/"/, "", $2); print $2; exit }' "$PROJECT_DIR/Cargo.toml"
}

map_deb_arch() {
    case "$(uname -m)" in
        x86_64|amd64) echo "amd64" ;;
        aarch64|arm64) echo "arm64" ;;
        *) uname -m ;;
    esac
}

write_desktop_file() {
    local desktop_path="$1"
    cat >"$desktop_path" <<EOF
[Desktop Entry]
Type=Application
Name=$APP_DISPLAY_NAME
Comment=$APP_SUMMARY
Exec=$APP_NAME
Icon=$APP_ID
Terminal=false
Categories=AudioVideo;Audio;
StartupWMClass=$APP_NAME
EOF
}

main() {
    need_cmd cargo
    need_cmd dpkg-deb
    need_cmd install

    if [[ ! -f "$ICON_PATH" ]]; then
        err "Icon not found: $ICON_PATH"
        exit 1
    fi

    APP_VERSION="$(read_version)"
    if [[ -z "${APP_VERSION:-}" ]]; then
        err "Cannot read version from Cargo.toml"
        exit 1
    fi
    DEB_ARCH="$(map_deb_arch)"

    mkdir -p "$DIST_DIR" "$BUILD_DIR"

    log "Building release binary..."
    (cd "$PROJECT_DIR" && cargo build --release)

    if [[ ! -x "$BINARY_PATH" ]]; then
        err "Binary missing: $BINARY_PATH"
        exit 1
    fi

    PKG_ROOT="$BUILD_DIR/deb/${APP_NAME}_${APP_VERSION}_${DEB_ARCH}"
    DEB_OUT="$DIST_DIR/${APP_NAME}_${APP_VERSION}_${DEB_ARCH}.deb"
    DESKTOP_PATH="$PKG_ROOT/usr/share/applications/${APP_ID}.desktop"
    APP_ICON_PATH="$PKG_ROOT/usr/share/icons/hicolor/256x256/apps/${APP_ID}.png"

    rm -rf "$PKG_ROOT"
    mkdir -p "$PKG_ROOT/DEBIAN" "$(dirname "$DESKTOP_PATH")" "$(dirname "$APP_ICON_PATH")" "$PKG_ROOT/usr/bin"

    cat >"$PKG_ROOT/DEBIAN/control" <<EOF
Package: $APP_NAME
Version: $APP_VERSION
Section: sound
Priority: optional
Architecture: $DEB_ARCH
Maintainer: $MAINTAINER
Description: $APP_SUMMARY
EOF

    install -m 0755 "$BINARY_PATH" "$PKG_ROOT/usr/bin/$APP_NAME"
    install -m 0644 "$ICON_PATH" "$APP_ICON_PATH"
    write_desktop_file "$DESKTOP_PATH"

    log "Creating DEB..."
    dpkg-deb --build --root-owner-group "$PKG_ROOT" "$DEB_OUT"
    log "OK: $DEB_OUT"
}

main "$@"
