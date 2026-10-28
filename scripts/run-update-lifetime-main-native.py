#!/usr/bin/env python3
"""Explicit Main27/schema19+22; historical Main25/Main26 profiles stay frozen."""
import importlib.util
from pathlib import Path
if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('lifetime_main_run', Path(__file__).with_name('run-update-service-main-native.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.main(lifetime=True)
