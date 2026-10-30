#!/usr/bin/env python3
"""Explicit production exclusion primitive plus two local OS cases; never install."""
import importlib.util
from pathlib import Path
s=importlib.util.spec_from_file_location('bootstrap_exclusion_run',Path(__file__).with_name('run-update-lifetime-spike-native.py'))
m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
if __name__=='__main__':
    m.main(bootstrap=True)
