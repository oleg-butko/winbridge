#!/usr/bin/env python3
"""Actual21 ->22 on a consistent copy; installed worker stays unchanged."""
import importlib.util
from pathlib import Path

if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('migration_runner', Path(__file__).with_name('run-update-service-migration-native.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.main(profile='Lifetime21To22')
