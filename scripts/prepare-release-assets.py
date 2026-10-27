#!/usr/bin/env python3
"""Create the public Flatpak manifest and checksums after building a release bundle."""
import hashlib
import json
from pathlib import Path
import tomllib

ROOT = Path(__file__).resolve().parents[1]
APP_ID = 'io.github.alesvomacka.CameraImport'


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def main():
    package = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']
    version, repository = package['version'], package['repository'].rstrip('/')
    dist = ROOT / 'dist'
    archive = dist / f'camera-import-applet-{version}.tar.gz'
    manifest = json.loads((ROOT / f'packaging/{APP_ID}.json').read_text())
    source = manifest['modules'][-1]['sources'][0]
    if digest(archive) != source['sha256']:
        raise SystemExit('Source archive checksum differs from the build manifest; rebuild with make flatpak-bundle.')
    source.pop('path', None)
    source['url'] = f'{repository}/releases/download/v{version}/{archive.name}'
    bundles = sorted(dist.glob(f'camera-import-applet-{version}-*.flatpak'))
    if not bundles:
        raise SystemExit('No Flatpak release bundle found; run make flatpak-bundle first.')
    public_manifest = dist / f'{APP_ID}.json'
    public_manifest.write_text(json.dumps(manifest, indent=2) + '\n')
    files = [*bundles, archive, public_manifest]
    (dist / 'SHA256SUMS').write_text(''.join(f'{digest(path)}  {path.name}\n' for path in files))
    print('Release manifest and SHA256SUMS refreshed.')


if __name__ == '__main__':
    main()
