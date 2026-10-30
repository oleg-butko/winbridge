#!/usr/bin/env python3
"""Readonly audit of actual24 ->25; never submit Windows commands."""
import importlib.util
from pathlib import Path

if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('migration_reader', Path(__file__).with_name('verify-update-service-migration-native.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.main(profile='Bootstrap24To25')
