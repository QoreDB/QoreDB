#!/usr/bin/env python3
# SPDX-License-Identifier: BUSL-1.1

from __future__ import annotations

import json
import os
import tempfile
import unittest
from pathlib import Path

from scripts import qore_ai_runtime


ROOT = Path(__file__).resolve().parent.parent
SPEC_PATH = ROOT / "packaging/qore-ai/runtime-build-v1.json"
MANIFEST_PATH = ROOT / "src-tauri/resources/qore-ai-local-manifest-v1.json"


class QoreAiRuntimeTests(unittest.TestCase):
    def test_spec_contains_exact_supported_matrix(self) -> None:
        spec = qore_ai_runtime.read_json(SPEC_PATH)

        qore_ai_runtime.validate_spec(spec)

        self.assertEqual(
            {
                (target["os"], target["architecture"])
                for target in qore_ai_runtime.matrix(spec)["target"]
            },
            qore_ai_runtime.SUPPORTED_TARGETS,
        )

    def test_tar_gz_packaging_is_deterministic(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source"
            source.mkdir()
            executable = source / "llama-server"
            executable.write_bytes(b"runtime")
            executable.chmod(0o755)
            (source / "LICENSE").write_text("MIT\n", encoding="utf-8")
            first = root / "first.tar.gz"
            second = root / "second.tar.gz"

            qore_ai_runtime.package(
                source, first, "tar_gz", "llama-b10087", 1_784_685_248
            )
            os.utime(executable, (2_000_000_000, 2_000_000_000))
            qore_ai_runtime.package(
                source, second, "tar_gz", "llama-b10087", 1_784_685_248
            )

            self.assertEqual(
                qore_ai_runtime.sha256_file(first),
                qore_ai_runtime.sha256_file(second),
            )

    def test_zip_packaging_is_deterministic(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source"
            source.mkdir()
            executable = source / "llama-server.exe"
            executable.write_bytes(b"runtime")
            first = root / "first.zip"
            second = root / "second.zip"

            qore_ai_runtime.package(source, first, "zip", "", 1_784_685_248)
            os.utime(executable, (2_000_000_000, 2_000_000_000))
            qore_ai_runtime.package(source, second, "zip", "", 1_784_685_248)

            self.assertEqual(
                qore_ai_runtime.sha256_file(first),
                qore_ai_runtime.sha256_file(second),
            )

    def test_reproducibility_check_rejects_different_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            first = root / "first"
            second = root / "second"
            first.write_bytes(b"one")
            second.write_bytes(b"two")

            with self.assertRaisesRegex(ValueError, "reproducibility check failed"):
                qore_ai_runtime.verify_pair(first, second)

    def test_manifest_promotion_requires_verified_six_target_set(self) -> None:
        spec = qore_ai_runtime.read_json(SPEC_PATH)
        manifest = qore_ai_runtime.read_json(MANIFEST_PATH)
        with tempfile.TemporaryDirectory() as temporary:
            artifacts_dir = Path(temporary)
            for target in spec["targets"]:
                artifact = artifacts_dir / target["artifact"]
                stage = artifacts_dir / f"stage-{target['id']}"
                stage.mkdir()
                executable_name = (
                    "llama-server.exe" if target["os"] == "windows" else "llama-server"
                )
                executable = stage / executable_name
                executable.write_bytes(target["id"].encode())
                executable.chmod(0o755)
                qore_ai_runtime.package(
                    stage,
                    artifact,
                    target["format"],
                    target["archive_root"],
                    spec["source"]["source_date_epoch"],
                )
                provenance = {
                    "artifact": {
                        "name": artifact.name,
                        "sha256": qore_ai_runtime.sha256_file(artifact),
                        "size": artifact.stat().st_size,
                    },
                    "build": {
                        "target": {
                            "os": target["os"],
                            "architecture": target["architecture"],
                        }
                    },
                    "source": spec["source"],
                }
                (artifacts_dir / f"{artifact.name}.provenance.json").write_text(
                    json.dumps(provenance), encoding="utf-8"
                )

            promoted = qore_ai_runtime.promote_manifest(spec, manifest, artifacts_dir)

            self.assertEqual(promoted["version"], spec["runtime"]["manifest_version"])
            for target in promoted["targets"]:
                self.assertTrue(
                    target["runtime"]["url"].startswith(
                        spec["runtime"]["release_base_url"]
                    )
                )


if __name__ == "__main__":
    unittest.main()
