#!/usr/bin/env python3
"""Real Git/archive regressions for source identity; no Windows runtime needed."""
import contextlib
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

spec = importlib.util.spec_from_file_location('export_source', Path(__file__).with_name('export-source.py'))
export = importlib.util.module_from_spec(spec)
spec.loader.exec_module(export)


class ExportSourceTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix='winbridge-export-test-')
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.root = self.base / 'repo'
        self.root.mkdir()
        root_patch = patch.object(export, 'ROOT', self.root)
        root_patch.start()
        self.addCleanup(root_patch.stop)
        self.git('init', '-q')
        self.name = ' Привет ✓.rs'
        (self.root / self.name).write_bytes(b'committed\x00bytes\n')
        self.git('add', '.')
        self.commit()
        self.head = self.git('rev-parse', 'HEAD').decode().strip()

    def git(self, *args):
        return subprocess.check_output(['git', '-C', str(self.root), *args])

    def commit(self):
        self.git('-c', 'user.name=Validation', '-c', 'user.email=validation@local',
                 'commit', '-qm', 'fixture')

    def test_clean_archive_has_exact_commit_files_and_checked_hashes(self):
        output = self.base / 'archives'
        with patch.object(sys, 'argv', ['export-source.py', '--output-directory', str(output)]), contextlib.redirect_stdout(io.StringIO()) as stdout:
            export.main()
        ready = json.loads(stdout.getvalue())
        archive = Path(ready['archivePath'])
        self.assertFalse(ready['dirty'])
        self.assertEqual(ready['sha256'], hashlib.sha256(archive.read_bytes()).hexdigest())
        with zipfile.ZipFile(archive) as package:
            manifest = json.loads(package.read('winbridge/winbridge-source-manifest.json'))
            self.assertEqual(manifest['sourceCommit'], self.head)
            self.assertFalse(manifest['dirty'])
            item, = manifest['files']
            data = package.read('winbridge/' + item['path'])
            self.assertEqual(data, b'committed\x00bytes\n')
            self.assertEqual(item['sha256'], hashlib.sha256(data).hexdigest())
            self.assertEqual(item['bytes'], len(data))

    def test_status_hidden_edits_and_deletions_are_dirty(self):
        for flag in ('--assume-unchanged', '--skip-worktree'):
            with self.subTest(flag=flag):
                self.git('update-index', flag, '--', self.name)
                (self.root / self.name).write_bytes(b'hidden edit')
                self.assertEqual(self.git('status', '--porcelain'), b'')
                commit, dirty, files = export.capture_sources()
                self.assertEqual(commit, self.head)
                self.assertTrue(dirty)
                self.assertEqual(files, [(self.name, b'hidden edit')])
                (self.root / self.name).unlink()
                self.assertTrue(export.capture_sources()[1])
                (self.root / self.name).write_bytes(b'committed\x00bytes\n')
                self.git('update-index', '--no-assume-unchanged', '--no-skip-worktree', '--', self.name)

    def test_captured_edit_restored_before_final_status_stays_dirty(self):
        original = Path.read_bytes

        def read(source):
            if source == self.root / self.name:
                source.write_bytes(b'edit during capture')
                data = original(source)
                source.write_bytes(b'committed\x00bytes\n')
                return data
            return original(source)

        with patch.object(Path, 'read_bytes', read):
            commit, dirty, files = export.capture_sources()
        self.assertEqual(commit, self.head)
        self.assertEqual(self.git('status', '--porcelain'), b'')
        self.assertTrue(dirty)
        self.assertEqual(files, [(self.name, b'edit during capture')])

    def test_new_file_is_included_and_marked_dirty(self):
        (self.root / 'new.rs').write_bytes(b'new source')
        _, dirty, files = export.capture_sources()
        self.assertTrue(dirty)
        self.assertIn(('new.rs', b'new source'), files)

    def test_head_moving_during_capture_retains_original_pin_and_marks_dirty(self):
        original = Path.read_bytes

        def read(source):
            data = original(source)
            if source == self.root / self.name:
                self.git('-c', 'user.name=Validation', '-c', 'user.email=validation@local',
                         'commit', '--allow-empty', '-qm', 'head moved')
            return data

        with patch.object(Path, 'read_bytes', read):
            commit, dirty, _ = export.capture_sources()
        self.assertEqual(commit, self.head)
        self.assertNotEqual(self.git('rev-parse', 'HEAD').decode().strip(), self.head)
        self.assertTrue(dirty)

    def test_sha256_repository_uses_actual_blob_object_format(self):
        other = self.base / 'sha256'
        other.mkdir()
        subprocess.run(['git', '-C', str(other), 'init', '-q', '--object-format=sha256'], check=True)
        with patch.object(export, 'ROOT', other):
            (other / 'source.rs').write_bytes(b'source')
            export.git('add', '.')
            export.git('-c', 'user.name=Validation', '-c', 'user.email=validation@local',
                       'commit', '-qm', 'sha256')
            commit, dirty, _ = export.capture_sources()
            self.assertEqual(len(commit), 64)
            self.assertFalse(dirty)
            (other / 'source.rs').write_bytes(b'changed')
            self.assertTrue(export.capture_sources()[1])

    def test_parent_checkout_identity_is_rejected(self):
        nested = self.root / 'generation'
        nested.mkdir()
        with patch.object(export, 'ROOT', nested), self.assertRaisesRegex(ValueError, 'own Git checkout'):
            export.capture_sources()

    def test_parent_symlink_is_rejected_before_reading_external_file(self):
        folder = self.root / 'src'
        folder.mkdir()
        (folder / 'main.rs').write_bytes(b'local')
        self.git('add', '.')
        self.commit()
        (folder / 'main.rs').unlink()
        folder.rmdir()
        external = self.base / 'external'
        external.mkdir()
        (external / 'main.rs').write_bytes(b'external')
        folder.symlink_to(external, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'contains a symlink'):
            export.capture_sources()


if __name__ == '__main__':
    unittest.main()
