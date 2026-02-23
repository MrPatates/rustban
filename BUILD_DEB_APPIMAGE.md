# Build Guide: DEB and AppImage

This guide covers only the Linux package formats used by RustBAN:

- `.deb`
- `.AppImage`

## Requirements

- `cargo`
- `dpkg-deb` (for `.deb`)
- `appimagetool` (for `.AppImage`)

## Build DEB

```bash
./build-deb.sh
```

Output:

- `dist/rustban_<version>_<arch>.deb`

## Build AppImage

```bash
./build-appimage.sh
```

Output:

- `dist/rustban-<version>-<arch>.AppImage`

## Notes

- Both scripts compile RustBAN in release mode before packaging.
- Both scripts use `app_icon.png` at the project root as the app icon source.
