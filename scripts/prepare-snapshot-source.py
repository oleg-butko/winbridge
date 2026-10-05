#!/usr/bin/env python3
"""Create-only local fixture source/config; publish with the actual Rust controller."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import time
import uuid
ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--config', type=Path, default=ROOT / '.winbridge/controller-snapshot.local.json')
p.add_argument('--base-config', type=Path, default=ROOT / '.winbridge/controller-session.local.json')
a = p.parse_args()
if not a.config.exists():
    base = json.loads(a.base_config.read_text())
    source = ROOT / '.winbridge/snapshot-source' / (time.strftime('%Y%m%d-%H%M%S') + '-' + uuid.uuid4().hex[:8])
    source.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(ROOT / 'fixtures/snapshot-project', source)
    for args in [('init', '-q'), ('add', '.'), ('-c', 'user.name=Fixture', '-c', 'user.email=fixture@local', 'commit', '-qm', 'Windows snapshot build fixture')]:
        subprocess.run(['git', '-C', str(source), *args], check=True)
    base['delivery'] = {'adapter': 'shared-folder', 'root': '/mnt/hgfs/shared', 'namespace': 'WinBridge', 'peerRoot': 'K:\\VMs\\shared'}
    base['projects'] = [pr for pr in base.get('projects', []) if pr['id'] != 'snapshot-fixture'] + [{'id': 'snapshot-fixture', 'sourceRoot': str(source), 'sourceMode': 'commit'}]
    with a.config.open('x') as f:
        json.dump(base, f, indent=2)
subprocess.run([str(ROOT / 'target/debug/winbridge-controller'), '--config', str(a.config), 'publish', 'snapshot-fixture', 'HEAD'], check=True)
print(f'Controller config preserved: {a.config}')
