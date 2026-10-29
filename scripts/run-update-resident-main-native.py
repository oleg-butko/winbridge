#!/usr/bin/env python3
"""Explicit Main29 current resident proof; no installation or historical relabel."""
import importlib.util
from pathlib import Path
if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('resident_main', Path(__file__).with_name('run-update-service-main-native.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.main(resident=True)
