#!/usr/bin/env python3
"""Readonly Main26 audit, without submitting/restarting Windows work."""
import importlib.util
from pathlib import Path
if __name__=='__main__':
    spec=importlib.util.spec_from_file_location('main_reader',Path(__file__).with_name('verify-update-service-main-native.py'))
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    module.main(controller=True)
