#!/usr/bin/env python3
"""Explicit Main28 partial Ready fault; never relabel historical Main27."""
import importlib.util
from pathlib import Path
if __name__ == '__main__':
    spec = importlib.util.spec_from_file_location('lifetime_ready_loss_main', Path(__file__).with_name('verify-update-service-main-native.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.main(ready_loss=True)
