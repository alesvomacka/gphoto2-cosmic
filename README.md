# Camera Import Applet for COSMIC

A small native COSMIC/libcosmic panel applet for copying photos from USB cameras via
`gphoto2`. Built from the [official COSMIC applet template](https://github.com/pop-os/cosmic-applet-template)

**This is entirely vibe coded using ChatGPT Sol and intended for personal use.**

## Features

- Choose a local destination through the desktop portal, or type an absolute path.
- Import RAW, JPEG, video, and sidecar files exposed by gphoto2; files go directly into the selected destination by default. **Preserve camera folder structure** optionally.
- Progress updates, current filename, elapsed time, average speed, approximate time remaining, completion status, and camera/I/O errors.
- **Import new only** skips existing destination paths.
- Optional `YYYY-MM-DD` subfolder using the **local date of export**, independent
  of the camera folder option. Both options are off by default.
## Install the Flatpak bundle

Download or copy the release bundle for your architecture, then run:

```sh
flatpak install --user ./camera-import-applet-0.1.0-x86_64.flatpak
```

Add **Camera Import Applet for COSMIC** to the COSMIC panel in Settings → Desktop → Panel → Applets.
For Niri, use the application launcher or `flatpak run io.github.alesvomacka.CameraImport --window`.
## Install in COSMIC

```sh
make install
```

This builds the release binary and installs it with both desktop entries under
`~/.local`. Ensure `~/.local/bin` is on the graphical session's `PATH` (log out and
back in after changing it). Open **COSMIC Settings → Desktop → Panel → Applets**
and add **Camera Import Applet for COSMIC** to the panel. A session restart may be needed for a new
applet entry to appear. The applet desktop entry is intentionally hidden from the
application launcher; the separate **Camera Import Applet for COSMIC** launcher opens a window.

For a system installation instead:

```sh
make build
sudo make install PREFIX=/usr/local
```

Remove with `make uninstall` (use the same `PREFIX` as for installation). A staged
package install is supported with `DESTDIR`.

## Niri

The native panel applet needs COSMIC Panel and its applet protocol. To use it with
Niri and another panel, launch the standalone window:

```sh
camera-import-applet --window
# Or, before installation:
cargo run --locked -- --window
```

The installed launcher does this automatically. An optional Niri binding is:

```kdl
binds {
    Mod+Shift+P { spawn "camera-import-applet" "--window"; }
}
```

Use the FileChooser backend configured for your Niri session, for example
`xdg-desktop-portal-gtk` alongside `xdg-desktop-portal`. The application uses
libcosmic widgets and follows the COSMIC theme; it does not implement a tray icon.