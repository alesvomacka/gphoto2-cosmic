# COSMIC Flatpak packaging

Third-party COSMIC panel applets are distributed through the
[official COSMIC Flatpak repository](https://github.com/pop-os/cosmic-flatpak),
which hosts desktop-specific packages unsuitable for Flathub. This project uses
that format: an applet desktop entry, AppStream metadata advertising
`com.system76.CosmicApplet`, and an offline source build with a locked Rust graph.
The Niri window launcher is included in the same package.

## Build a shareable bundle

Requirements: Flatpak 1.16+, Flatpak Builder, Python 3.11+, Rust/Cargo 1.93+, and
network access for the initial dependency downloads. For Fedora:

```sh
sudo dnf install flatpak flatpak-builder python3 cargo rust make
flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
make flatpak-bundle
```

This creates:

- `dist/camera-import-applet-0.1.0-<architecture>.flatpak`: installable bundle.
- `dist/camera-import-applet-0.1.0.tar.gz`: vendored build sources.
- `dist/flatpak-repo/`: local OSTree repository.
- `dist/io.github.alesvomacka.CameraImport.json`: release manifest with a public source-archive URL.
- `dist/SHA256SUMS`: checksums for the three release assets.

The build installs the Freedesktop 26.08 SDK and Rust extension into the builder's
user Flatpak installation if needed. Those are build dependencies; users installing
the bundle only need the matching platform runtime, fetched using the runtime
repository embedded in the bundle. Rust dependencies build offline, and upstream
C dependency archives have SHA-256 checksums in the manifest. `make flatpak-source`
regenerates the vendored archive and its manifest checksum after source changes.
`FLATPAK_BUILDER` can override the builder command. `FLATPAK_USER_DIR` can isolate
SDK/runtime downloads from a normal user installation.

Bundled versions: gphoto2 2.5.32, libgphoto2 2.5.34 (standard camera-driver set,
including Nikon PTP), libusb 1.0.30, and popt 1.19. Users do not need host gphoto2.
The Freedesktop runtime supplies the other shared libraries and `date`.

## Install and run

Copy the `.flatpak` file to another machine of the same architecture, then:

```sh
flatpak install --user ./camera-import-applet-0.1.0-x86_64.flatpak
```

Add **Camera Import Applet for COSMIC** in COSMIC Settings → Desktop → Panel → Applets. Log out and
back in if the panel has not refreshed its applet list. On Niri, use the **Camera Import Applet for COSMIC** application launcher or:

```sh
flatpak run io.github.alesvomacka.CameraImport --window
```

To uninstall:

```sh
flatpak uninstall --user io.github.alesvomacka.CameraImport
```

If a native installation is also present, uninstall it before adding the Flatpak
applet to avoid duplicate desktop entries. Existing native files are not removed
by the Flatpak installer.

## Sandbox access

- Wayland and rendering access for the native interface.
- Raw USB access (`--device=usb`, requiring Flatpak 1.16+) for gphoto2/libusb.
  This implementation does not use the USB portal and does not bypass host udev
  permissions or competing camera applications.
- Pictures read/write access for the default destination; other destinations are
  granted by the folder chooser portal. Typing a path outside granted folders
  does not grant access; use **Choose folder** for those destinations.
- Read-only COSMIC configuration and access to CosmicSettingsDaemon for theming.
- No network, full-home/full-host filesystem, unrestricted D-Bus, or host command
  execution permissions. gphoto2 runs inside the package.

## Publish on personal GitHub

The app ID is `io.github.alesvomacka.CameraImport`, using the owner's GitHub
namespace. Publication is through personal GitHub Releases; this project is not
submitted to the COSMIC applet repository.

1. Push the source to [alesvomacka/gphoto2-cosmic](https://github.com/alesvomacka/gphoto2-cosmic).
   The package metadata links to this repository and its issue tracker.
2. Run `make flatpak-bundle`, then create a GitHub release such as `v0.1.0`.
3. Attach the `.flatpak` bundle, vendored source archive, Flatpak manifest, and
   `SHA256SUMS` from `dist/`. The bundle target generates the release manifest
   with a versioned GitHub source URL and refreshes all checksums automatically.
4. Users download and install the bundle with the command above. Repeat that
   command with a newer bundle to update. GitHub release bundles do not provide
   automatic updates; a signed Flatpak remote would be needed for that.

If you installed the earlier test bundle with the placeholder ID, remove its
panel entry and uninstall it before installing this renamed package:

```sh
flatpak uninstall --user io.github.camera_import.Applet
```

After installing the new bundle, add Camera Import Applet for COSMIC back to the panel. The old and
new IDs are distinct applications; Flatpak does not rename existing installations.

The source project keeps a single canonical libcosmic git source to allow Cargo
vendoring without duplicate crate versions. `Cargo.lock` pins its exact commit;
all supported build commands use `--locked`.
