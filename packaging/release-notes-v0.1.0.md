Initial release of **Camera Import Applet for COSMIC**.

Import RAW photos, JPEGs, videos, and sidecars from USB cameras using a native COSMIC panel applet. Includes camera selection, a folder chooser, new-only importing, optional export-date and camera folders, detailed progress, and cancellation. Downloads share one gphoto2 process; camera originals are kept.

The Flatpak includes gphoto2 2.5.32 and libgphoto2 2.5.34, supporting Nikon, Canon, Sony, Fujifilm, Olympus, Panasonic, and many other models. It requires Flatpak 1.16+ and a Wayland desktop. The attached binary targets **x86_64**.

Install the attached bundle:

```sh
flatpak install --user ./camera-import-applet-0.1.0-x86_64.flatpak
```

Add **Camera Import Applet for COSMIC** through COSMIC Settings → Desktop → Panel → Applets. For Niri or another Wayland desktop, use the installed application launcher or:

```sh
flatpak run io.github.alesvomacka.CameraImport --window
```

Use **Choose folder** to grant access to destinations outside Pictures. “New only” compares destination paths; it does not maintain an import history across different date folders. Progress can pause while gphoto2 buffers a camera file, and the time remaining is approximate.

The `SHA256SUMS` attachment covers the Flatpak, the vendored Rust/app source archive, and the public Flatpak build manifest. After downloading those three files and the checksum file into the same directory, verify them with `sha256sum -c SHA256SUMS`. Upstream camera-tool sources and checksums are listed in the manifest.

This is a personal project built with AI assistance. It is distributed through this GitHub release and has not been submitted to the official COSMIC applet repository.
