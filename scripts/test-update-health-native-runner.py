#!/usr/bin/env python3
"""Archive admission must reject mutations before remote extraction/execution."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

spec=importlib.util.spec_from_file_location('health',Path(__file__).with_name('validate-update-health-actor-native.py'))
health=importlib.util.module_from_spec(spec);spec.loader.exec_module(health)

class ArchiveTests(unittest.TestCase):
    def package(self,root,mutation=''):
        path=root/'source.zip';content=b'fn main(){}'
        manifest={'schemaVersion':1,'dirty':False,'includesBuiltUi':True,'sourceCommit':'a'*40,'files':[{'path':'src/main.rs','bytes':len(content),'sha256':hashlib.sha256(content).hexdigest()}]}
        if mutation=='dirty':manifest['dirty']=True
        with zipfile.ZipFile(path,'w') as z:
            z.writestr('winbridge/winbridge-source-manifest.json',json.dumps(manifest))
            z.writestr('winbridge/src/main.rs',b'corrupt' if mutation=='hash' else content)
            if mutation=='extra':z.writestr('winbridge/unlisted',b'unsafe')
            if mutation=='traversal':z.writestr('winbridge/../escape.ps1',b'unsafe')
            if mutation=='case':z.writestr('WINBRIDGE/src/main.rs',content)
            if mutation=='symlink':
                e=zipfile.ZipInfo('winbridge/link');e.create_system=3;e.external_attr=(0o120777<<16);z.writestr(e,b'../../outside')
        return path
    def test_clean_package_keeps_exact_source_archive_and_manifest_hashes(self):
        with tempfile.TemporaryDirectory() as d:
            p=self.package(Path(d));pin=health.archive_pin(p)
            self.assertEqual(pin['archiveSha256'],hashlib.sha256(p.read_bytes()).hexdigest());self.assertEqual(pin['sourceCommit'],'a'*40)
    def test_mutated_or_unsafe_zip_never_reaches_remote_execution(self):
        for kind in ['dirty','hash','extra','traversal','case','symlink']:
            with self.subTest(kind=kind),tempfile.TemporaryDirectory() as d:
                with self.assertRaises((ValueError,KeyError)):
                    health.archive_pin(self.package(Path(d),kind))
    def test_symlink_source_is_not_admitted_as_immutable_publication(self):
        with tempfile.TemporaryDirectory() as d:
            p=self.package(Path(d));link=Path(d)/'linked.zip';link.symlink_to(p)
            with self.assertRaises(ValueError):health.archive_pin(link)

class ReceiptBytesTests(unittest.TestCase):
    def receipt(self):
        return dict(schemaVersion=1, updateId='u', intentSha256='h', parentRevision=2,
                    completedAtMs=3, directoryIdentity='d', files=[dict(path='K:\\é', bytes=7,
                    sha256='s', fileIdentity='i', querySupported=True)])

    def test_preimage_hash_uses_rust_field_order_and_utf8_after_json_key_reordering(self):
        receipt = self.receipt()
        receipt['files'][0] = dict(reversed(list(receipt['files'][0].items())))
        receipt = dict(reversed(list(receipt.items())))
        expected = '{"schemaVersion":1,"updateId":"u","intentSha256":"h","parentRevision":2,"completedAtMs":3,"directoryIdentity":"d","files":[{"path":"K:\\\\é","bytes":7,"sha256":"s","fileIdentity":"i","querySupported":true}]}'.encode()
        self.assertEqual(health.preimages_receipt_bytes(receipt), expected)

    def test_unknown_receipt_or_image_fields_are_never_silently_dropped_from_hash(self):
        for image in [False, True]:
            receipt = self.receipt()
            (receipt['files'][0] if image else receipt)['unknown'] = True
            with self.assertRaises(ValueError): health.preimages_receipt_bytes(receipt)

class BackupReasonTests(unittest.TestCase):
    def test_cleanup_stderr_controls_match_saved_reason_without_dropping_format_characters(self):
        self.assertEqual(health.backup_failure_reason('Artifact query failed; stderr=WinBridge: The file exists. (os error 80)\n'),
                         'Artifact query failed; stderr=WinBridge: The file exists. (os error 80)')
        self.assertEqual(health.backup_failure_reason('A\n\r\t\x7f\x85B\u200d🧪'), 'AB\u200d🧪')

    def test_reason_bound_counts_scalars_instead_of_utf8_bytes_or_utf16_units(self):
        self.assertEqual(health.backup_failure_reason('🧪'*901), '🧪'*900)

if __name__=='__main__':unittest.main()
