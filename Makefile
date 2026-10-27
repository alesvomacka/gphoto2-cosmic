PREFIX ?= $(HOME)/.local
DESTDIR ?=

.PHONY: build check install uninstall
build:
	cargo build --release --locked
check:
	cargo fmt --check
	cargo test --locked
	cargo clippy --locked --all-targets -- -D warnings
	python3 tests/test_backend.py
target/release/camera-import-applet: Cargo.toml Cargo.lock $(wildcard src/*.rs)
	cargo build --release --locked
install: target/release/camera-import-applet
	install -Dm755 target/release/camera-import-applet $(DESTDIR)$(PREFIX)/bin/camera-import-applet
	install -Dm644 resources/io.github.alesvomacka.CameraImport.desktop $(DESTDIR)$(PREFIX)/share/applications/io.github.alesvomacka.CameraImport.desktop
	install -Dm644 resources/io.github.alesvomacka.CameraImport.Window.desktop $(DESTDIR)$(PREFIX)/share/applications/io.github.alesvomacka.CameraImport.Window.desktop
	install -Dm644 resources/io.github.alesvomacka.CameraImport.metainfo.xml $(DESTDIR)$(PREFIX)/share/metainfo/io.github.alesvomacka.CameraImport.metainfo.xml
	install -Dm644 resources/io.github.alesvomacka.CameraImport.svg $(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/io.github.alesvomacka.CameraImport.svg
	install -Dm644 resources/io.github.alesvomacka.CameraImport-symbolic.svg $(DESTDIR)$(PREFIX)/share/icons/hicolor/symbolic/apps/io.github.alesvomacka.CameraImport-symbolic.svg
	install -Dm644 LICENSE $(DESTDIR)$(PREFIX)/share/licenses/camera-import-applet/LICENSE
	install -Dm644 NOTICE $(DESTDIR)$(PREFIX)/share/licenses/camera-import-applet/NOTICE
uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/camera-import-applet $(DESTDIR)$(PREFIX)/share/applications/io.github.alesvomacka.CameraImport.desktop $(DESTDIR)$(PREFIX)/share/applications/io.github.alesvomacka.CameraImport.Window.desktop $(DESTDIR)$(PREFIX)/share/applications/io.github.alesvomacka.CameraImport.Window.desktop
	rm -f $(DESTDIR)$(PREFIX)/share/metainfo/io.github.alesvomacka.CameraImport.metainfo.xml $(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/io.github.alesvomacka.CameraImport.svg $(DESTDIR)$(PREFIX)/share/icons/hicolor/symbolic/apps/io.github.alesvomacka.CameraImport-symbolic.svg
	rm -f $(DESTDIR)$(PREFIX)/share/licenses/camera-import-applet/LICENSE $(DESTDIR)$(PREFIX)/share/licenses/camera-import-applet/NOTICE

.PHONY: flatpak-source flatpak flatpak-bundle
FLATPAK_BUILDER ?= flatpak-builder
flatpak-source:
	python3 scripts/prepare-flatpak.py
flatpak: flatpak-source
	$(FLATPAK_BUILDER) --user --force-clean --repo=dist/flatpak-repo --install-deps-from=flathub target/flatpak packaging/io.github.alesvomacka.CameraImport.json
flatpak-bundle: flatpak
	flatpak build-bundle --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo dist/flatpak-repo dist/camera-import-applet-$(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml)-$(shell flatpak --default-arch).flatpak io.github.alesvomacka.CameraImport stable
	python3 scripts/prepare-release-assets.py
