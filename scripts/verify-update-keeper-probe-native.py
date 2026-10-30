#!/usr/bin/env python3
"""Explicit original-child IPC/Job/handle probe unit; never install."""
import importlib.util
from pathlib import Path
s=importlib.util.spec_from_file_location('bootstrap_exclusion_verify',Path(__file__).with_name('verify-update-lifetime-spike-native.py'))
m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
if __name__=='__main__':
    import argparse,json
    p=argparse.ArgumentParser(description='Readonly exact original-child probe audit; no Windows RPC')
    p.add_argument('--run-info',required=True,type=Path);p.add_argument('--source-archive',required=True,type=Path)
    a=p.parse_args();print(json.dumps(m.verify(a.run_info,a.source_archive,keeper_probe=True),indent=2))
