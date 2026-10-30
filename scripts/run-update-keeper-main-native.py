#!/usr/bin/env python3
"""Explicit Main31 worker19/updater24 keeper retirement; no installed update."""
import importlib.util
from pathlib import Path
s=importlib.util.spec_from_file_location('keeper_main_run',Path(__file__).with_name('run-update-service-main-native.py'))
m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
if __name__=='__main__':m.main(keeper=True)
