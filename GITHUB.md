# RustBAN for GitHub

RustBAN is a focused desktop app for configuring **VBAN streams on PipeWire**.

It is intentionally minimal and centered on VBAN workflows:

- configure VBAN Send and VBAN Recv entries
- tune IP/port, stream names, format/rate/channels
- set node metadata (`node.name`, `node.description`, `node.always-process`)
- save settings in TOML
- generate PipeWire fragments from the GUI

RustBAN does not include full mixer features by design.

For packaging instructions (`.deb` and `.AppImage`), see `BUILD_DEB_APPIMAGE.md`.
