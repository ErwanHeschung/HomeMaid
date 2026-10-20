#!/usr/bin/env python3
"""Releases the official plugins whose version is not released yet: for each plugins/<dir> with
a registry.json (its listing), builds the .wasm, signs it with minisign, and creates the GitHub
release <id>-v<version> with the .wasm, its .minisig and entry.json (the registry entry, which
HomeMaid-plugins picks up and proposes to its index).

Run by .github/workflows/release-plugins.yml on main; locally too:
    MINISIGN_KEY=<secret key file> python3 tools/release_plugins.py [--dry-run]    (from core/)
    python3 tools/release_plugins.py --pending    (only lists the tags to release)
Needs cargo with wasm32-wasip2, minisign, gh. A version is released once: bump it in Cargo.toml.
"""
import hashlib, json, os, pathlib, re, subprocess, sys

CORE = pathlib.Path(__file__).resolve().parent.parent
KEY = (CORE / 'plugins/minisign.pub').read_text().strip().splitlines()[-1]
DRY = '--dry-run' in sys.argv
PENDING = '--pending' in sys.argv


def run(*args, **kw):
    print('+', ' '.join(args), flush=True)
    return subprocess.run(args, check=True, cwd=CORE, **kw)


def released(tag):
    return subprocess.run(['gh', 'release', 'view', tag], cwd=CORE, capture_output=True).returncode == 0


# The plugin-api the SDK builds for (`1.2.0` -> `1.2`): the oldest core minor that can run it.
api = re.search(r'API_VERSION: &str = "(\d+\.\d+)', (CORE / 'plugin-api/src/lib.rs').read_text()).group(1)
repo = os.environ.get('GITHUB_REPOSITORY', 'ErwanHeschung/HomeMaid')
for listing in sorted(CORE.glob('plugins/*/registry.json')):
    entry = json.loads(listing.read_text())
    cargo = (listing.parent / 'Cargo.toml').read_text()
    crate = re.search(r'^name = "(.+)"', cargo, re.M).group(1)
    version = re.search(r'^version = "(.+)"', cargo, re.M).group(1)
    tag = f"{entry['id']}-v{version}"
    if released(tag):
        if not PENDING:
            print(f'{tag}: already released')
        continue
    if PENDING:
        print(tag)
        continue
    run('cargo', 'build', '--release', '-p', crate, '--target', 'wasm32-wasip2')
    wasm = CORE / 'target/wasm32-wasip2/release' / f"{crate.replace('-', '_')}.wasm"
    run('minisign', '-S', '-s', os.environ['MINISIGN_KEY'], '-m', str(wasm), '-t', f'{tag} plugin-api {api}', stdin=subprocess.DEVNULL)
    entry.update(key=KEY, versions=[{
        'version': version,
        'api': api,
        'permissions': entry.pop('permissions'),
        'url': f'https://github.com/{repo}/releases/download/{tag}/{wasm.name}',
        'sha256': hashlib.sha256(wasm.read_bytes()).hexdigest(),
        'signature': pathlib.Path(f'{wasm}.minisig').read_text(),
    }])
    out = CORE / 'target/releases' / tag / 'entry.json'
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(entry, indent=2) + '\n')
    if DRY:
        print(f'{tag}: built and signed, not released (--dry-run): {out}')
        continue
    run('gh', 'release', 'create', tag, str(wasm), f'{wasm}.minisig', str(out), '--title', f"{entry['name']} {version}",
        '--notes', f"Plugin `{entry['id']}` {version}, plugin-api {api}. Signed with the key of core/plugins/minisign.pub.")
