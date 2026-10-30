"""Explicit retained worker17 inputs shared by native validation drivers."""
from pathlib import PureWindowsPath
import re


WORKER17_SHA256 = '536d86378b5c5adfc98b135c404e322d1dc9cc7dd19fb20941ca9b3f6686a712'


def worker17_reference(value):
    if not isinstance(value, str) or not value:
        raise RuntimeError('Explicit retained worker17 path required; installed version is not a reference')
    path = PureWindowsPath(value)
    if not (path.is_absolute() and path.drive.lower() == 'k:'
            and '..' not in path.parts and path.name.lower() == 'winbridge-worker.exe'):
        raise RuntimeError('Worker17 reference must be an absolute K: artifact path without traversal')
    if path == PureWindowsPath(r'K:\_proj-26\winbridge\runtime\winbridge-worker.exe'):
        raise RuntimeError('Worker17 reference must be retained separately from the installed runtime')
    return path


def reference_for_profile(profile, value, digest):
    if profile in ('worker19-updater20','worker19-updater21','worker19-updater22','worker19-updater23','worker19-updater24'):
        if not re.fullmatch(r'[0-9a-f]{64}', digest or ''):
            raise RuntimeError('Explicit released worker17 hash required for separate cold migration gate')
        return worker17_reference(value)
    if profile != 'worker17-updater20' or value is not None or digest is not None:
        raise RuntimeError('Cold worker17-to19 migration belongs only to the worker19 profile')
    return None
