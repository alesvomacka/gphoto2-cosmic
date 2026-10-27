#!/usr/bin/env python3
"""Create a locked, vendored source archive and update the local Flatpak manifest."""
import gzip
import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / 'packaging/io.github.alesvomacka.CameraImport.json'

def main():
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
    dist = ROOT / 'dist'
    dist.mkdir(exist_ok=True)
    archive = dist / f'camera-import-applet-{version}.tar.gz'
    (ROOT / 'target').mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='flatpak-source-', dir=ROOT / 'target') as temporary:
        vendor = Path(temporary) / 'vendor'
        print('Vendoring locked Rust dependencies…', flush=True)
        result = subprocess.run(['cargo', 'vendor', '--locked', str(vendor)], cwd=ROOT,
                                capture_output=True, text=True)
        if result.returncode:
            raise SystemExit(result.stderr)
        config = re.sub(r'^directory = .*$', 'directory = "vendor"', result.stdout, flags=re.M)
        prefix = f'camera-import-applet-{version}'
        def normalize(info):
            info.uid = info.gid = info.mtime = 0
            info.uname = info.gname = ''
            return info
        print('Creating vendored build archive…', flush=True)
        with archive.open('wb') as output, gzip.GzipFile(fileobj=output, mode='wb', mtime=0, filename='') as zipped:
            with tarfile.open(fileobj=zipped, mode='w|') as tar:
                for name in ('Cargo.toml', 'Cargo.lock', 'Makefile', 'README.md', 'LICENSE', 'NOTICE', 'src', 'resources', 'tests'):
                    tar.add(ROOT / name, arcname=f'{prefix}/{name}', filter=normalize)
                tar.add(vendor, arcname=f'{prefix}/vendor', filter=normalize)
                data = config.encode()
                info = tarfile.TarInfo(f'{prefix}/.cargo/config.toml')
                info.size, info.mode = len(data), 0o644
                tar.addfile(info, io.BytesIO(data))
    digest = hashlib.file_digest(archive.open('rb'), 'sha256').hexdigest()
    manifest = json.loads(MANIFEST.read_text())
    manifest['modules'][-1]['sources'] = [{
        'type': 'archive', 'path': f'../dist/{archive.name}', 'sha256': digest,
    }]
    MANIFEST.write_text(json.dumps(manifest, indent=2) + '\n')
    print(f'Created {archive.relative_to(ROOT)}\nSHA256: {digest}')

if __name__ == '__main__':
    main()
