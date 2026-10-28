#!/usr/bin/env python3
"""One Main26/schema19+21 fixture in stable test runtime; never installed update."""
import importlib.util
from pathlib import Path
if __name__=='__main__':
    spec=importlib.util.spec_from_file_location('main_runner',Path(__file__).with_name('run-update-service-main-native.py'))
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    module.main(controller=True)
