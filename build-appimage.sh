#!/usr/bin/env bash
set -euo pipefail

APP_NAME="rustban"
APP_ID="com.rustban.app"
APP_DISPLAY_NAME="RustBAN"
APP_SUMMARY="VBAN-only PipeWire control panel"

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR"
DIST_DIR="$PROJECT_DIR/dist"
BUILD_DIR="$DIST_DIR/.build"
TARGET_DIR="$PROJECT_DIR/target/release"

BINARY_PATH="$TARGET_DIR/$APP_NAME"
ICON_PATH="$PROJECT_DIR/app_icon.png"

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

resolve_appimagetool() {
    local local_tool="$PROJECT_DIR/.tools/appimagetool"

    if command -v appimagetool >/dev/null 2>&1; then
        printf '%s\n' "appimagetool"
        return 0
    fi

    if [[ -x "$local_tool" ]]; then
        printf '%s\n' "$local_tool"
        return 0
    fi

    err "Missing command: appimagetool"
    err "Install it system-wide or place an executable at: $local_tool"
    exit 1
}

read_version() {
    awk -F ' *= *' '/^version *=/ { gsub(/"/, "", $2); print $2; exit }' "$PROJECT_DIR/Cargo.toml"
}

map_appimage_arch() {
    case "$(uname -m)" in
        x86_64|amd64) echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
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
    need_cmd install
    APPIMAGETOOL_CMD="$(resolve_appimagetool)"

    if [[ ! -f "$ICON_PATH" ]]; then
        err "Icon not found: $ICON_PATH"
        exit 1
    fi

    APP_VERSION="$(read_version)"
    if [[ -z "${APP_VERSION:-}" ]]; then
        err "Cannot read version from Cargo.toml"
        exit 1
    fi
    APPIMAGE_ARCH="$(map_appimage_arch)"

    mkdir -p "$DIST_DIR" "$BUILD_DIR"

    log "Building release binary..."
    (cd "$PROJECT_DIR" && cargo build --release)

    if [[ ! -x "$BINARY_PATH" ]]; then
        err "Binary missing: $BINARY_PATH"
        exit 1
    fi

    APPDIR="$BUILD_DIR/AppDir"
    APPIMAGE_OUT="$DIST_DIR/${APP_NAME}-${APP_VERSION}-${APPIMAGE_ARCH}.AppImage"
    DESKTOP_USR="$APPDIR/usr/share/applications/${APP_ID}.desktop"
    ICON_USR="$APPDIR/usr/share/icons/hicolor/256x256/apps/${APP_ID}.png"

    rm -rf "$APPDIR"
    mkdir -p "$(dirname "$DESKTOP_USR")" "$(dirname "$ICON_USR")" "$APPDIR/usr/bin"

    install -m 0755 "$BINARY_PATH" "$APPDIR/usr/bin/$APP_NAME"
    install -m 0644 "$ICON_PATH" "$ICON_USR"
    write_desktop_file "$DESKTOP_USR"

    cat >"$APPDIR/AppRun" <<EOF
#!/usr/bin/env bash
set -euo pipefail
HERE="\$(cd -- "\$(dirname -- "\${BASH_SOURCE[0]}")" && pwd)"
exec "\$HERE/usr/bin/$APP_NAME" "\$@"
EOF
    chmod +x "$APPDIR/AppRun"

    cp "$DESKTOP_USR" "$APPDIR/${APP_ID}.desktop"
    cp "$ICON_USR" "$APPDIR/${APP_ID}.png"

    log "Creating AppImage..."
    ARCH="$APPIMAGE_ARCH" "$APPIMAGETOOL_CMD" "$APPDIR" "$APPIMAGE_OUT"
    chmod +x "$APPIMAGE_OUT"
    log "OK: $APPIMAGE_OUT"
}

main "$@"
