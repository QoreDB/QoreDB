#!/usr/bin/env python3
# SPDX-License-Identifier: BUSL-1.1

"""Build-catalog and deterministic packaging helpers for Qore AI Local."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
import stat
import tarfile
import tempfile
import zipfile
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath
from typing import Any

SUPPORTED_TARGETS = {
    ("macos", "aarch64"),
    ("macos", "x86_64"),
    ("linux", "aarch64"),
    ("linux", "x86_64"),
    ("windows", "aarch64"),
    ("windows", "x86_64"),
}


def read_json(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as file:
        value = json.load(file)
    if not isinstance(value, dict):
        raise ValueError(f"{path} must contain a JSON object")
    return value


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="\n") as file:
        json.dump(value, file, ensure_ascii=False, indent=2, sort_keys=True)
        file.write("\n")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def manifest_version_key(value: str) -> tuple[datetime, int]:
    manifest_date, revision = value.split(".", maxsplit=1)
    return datetime.strptime(manifest_date, "%Y-%m-%d"), int(revision)


def validate_spec(spec: dict[str, Any]) -> None:
    if spec.get("schema_version") != 1:
        raise ValueError("runtime build spec schema_version must be 1")

    source = spec.get("source", {})
    commit = source.get("commit", "")
    if len(commit) != 40 or any(
        character not in "0123456789abcdef" for character in commit
    ):
        raise ValueError("llama.cpp source commit must be a full lowercase Git SHA")
    if not isinstance(source.get("source_date_epoch"), int):
        raise ValueError("source_date_epoch must be an integer")

    runtime = spec.get("runtime", {})
    manifest_version = runtime.get("manifest_version", "")
    try:
        manifest_date, manifest_revision = manifest_version.split(".", maxsplit=1)
        datetime.strptime(manifest_date, "%Y-%m-%d")
    except (AttributeError, ValueError):
        manifest_revision = ""
    if not manifest_revision.isdigit():
        raise ValueError("manifest_version must use the YYYY-MM-DD.revision format")
    boringssl_commit = runtime.get("boringssl_commit", "")
    if (
        len(boringssl_commit) != 40
        or any(character not in "0123456789abcdef" for character in boringssl_commit)
    ):
        raise ValueError("BoringSSL commit must be a full lowercase Git SHA")

    targets = spec.get("targets")
    if not isinstance(targets, list):
        raise ValueError("runtime build spec targets must be an array")
    actual = {(target.get("os"), target.get("architecture")) for target in targets}
    if len(targets) != 6 or actual != SUPPORTED_TARGETS:
        raise ValueError("runtime build spec must contain exactly the six supported targets")

    ids: set[str] = set()
    artifacts: set[str] = set()
    for target in targets:
        target_id = target.get("id")
        artifact = target.get("artifact")
        if not isinstance(target_id, str) or not target_id or target_id in ids:
            raise ValueError("target ids must be non-empty and unique")
        if not isinstance(artifact, str) or not artifact or artifact in artifacts:
            raise ValueError("artifact names must be non-empty and unique")
        ids.add(target_id)
        artifacts.add(artifact)
        expected_suffix = ".zip" if target.get("format") == "zip" else ".tar.gz"
        if target.get("format") not in {"zip", "tar_gz"} or not artifact.endswith(
            expected_suffix
        ):
            raise ValueError(f"invalid archive format for {target_id}")
        runtime_path = PurePosixPath(target.get("runtime_relative_path", ""))
        if runtime_path.is_absolute() or ".." in runtime_path.parts:
            raise ValueError(f"unsafe runtime_relative_path for {target_id}")
        if not target.get("runner") or not isinstance(target.get("cmake_defines"), list):
            raise ValueError(f"incomplete build configuration for {target_id}")


def matrix(spec: dict[str, Any]) -> dict[str, list[dict[str, Any]]]:
    validate_spec(spec)
    source = spec["source"]
    runtime = spec["runtime"]
    targets = []
    for target in spec["targets"]:
        item = dict(target)
        item["source"] = source
        item["runtime_version"] = runtime["version"]
        item["common_cmake_defines"] = runtime["common_cmake_defines"]
        item["windows_ninja_version"] = runtime["windows_ninja_version"]
        targets.append(item)
    return {"target": targets}


def normalized_mode(path: Path) -> int:
    if path.is_dir():
        return 0o755
    executable = bool(path.stat().st_mode & stat.S_IXUSR)
    if path.suffix.lower() in {".exe", ".dll", ".dylib", ".so"}:
        executable = True
    return 0o755 if executable else 0o644


def archive_entries(source: Path) -> list[Path]:
    return sorted(
        (path for path in source.rglob("*")),
        key=lambda path: path.relative_to(source).as_posix(),
    )


def tar_info(path: Path, archive_name: str, epoch: int) -> tarfile.TarInfo:
    info = tarfile.TarInfo(archive_name)
    info.mtime = epoch
    info.uid = 0
    info.gid = 0
    info.uname = ""
    info.gname = ""
    info.mode = normalized_mode(path)
    if path.is_symlink():
        info.type = tarfile.SYMTYPE
        info.linkname = os.readlink(path)
    elif path.is_dir():
        info.type = tarfile.DIRTYPE
    else:
        info.type = tarfile.REGTYPE
        info.size = path.stat().st_size
    return info


def package_tar_gz(source: Path, output: Path, root: str, epoch: int) -> None:
    with output.open("wb") as raw:
        with gzip.GzipFile(
            filename="", mode="wb", fileobj=raw, mtime=epoch, compresslevel=9
        ) as gz:
            with tarfile.open(fileobj=gz, mode="w", format=tarfile.GNU_FORMAT) as archive:
                if root:
                    root_info = tarfile.TarInfo(root)
                    root_info.type = tarfile.DIRTYPE
                    root_info.mode = 0o755
                    root_info.mtime = epoch
                    archive.addfile(root_info)
                for path in archive_entries(source):
                    relative = path.relative_to(source).as_posix()
                    archive_name = f"{root}/{relative}" if root else relative
                    info = tar_info(path, archive_name, epoch)
                    if info.isfile():
                        with path.open("rb") as file:
                            archive.addfile(info, file)
                    else:
                        archive.addfile(info)


def package_zip(source: Path, output: Path, root: str, epoch: int) -> None:
    timestamp = datetime.fromtimestamp(epoch, timezone.utc)
    zip_date = (
        max(timestamp.year, 1980),
        timestamp.month,
        timestamp.day,
        timestamp.hour,
        timestamp.minute,
        timestamp.second - (timestamp.second % 2),
    )
    with zipfile.ZipFile(
        output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
        for path in archive_entries(source):
            relative = path.relative_to(source).as_posix()
            archive_name = f"{root}/{relative}" if root else relative
            if path.is_dir():
                archive_name += "/"
            info = zipfile.ZipInfo(archive_name, zip_date)
            info.create_system = 3
            info.external_attr = normalized_mode(path) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            if path.is_symlink():
                info.external_attr = (stat.S_IFLNK | 0o777) << 16
                archive.writestr(info, os.readlink(path).encode())
            elif path.is_dir():
                archive.writestr(info, b"")
            else:
                archive.writestr(info, path.read_bytes())


def package(source: Path, output: Path, archive_format: str, root: str, epoch: int) -> None:
    if not source.is_dir():
        raise ValueError(f"staging directory does not exist: {source}")
    if not any(source.iterdir()):
        raise ValueError("staging directory is empty")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        prefix=f"{output.name}.", suffix=".tmp", dir=output.parent, delete=False
    ) as temporary:
        temporary_path = Path(temporary.name)
    try:
        if archive_format == "tar_gz":
            package_tar_gz(source, temporary_path, root, epoch)
        elif archive_format == "zip":
            package_zip(source, temporary_path, root, epoch)
        else:
            raise ValueError(f"unsupported archive format: {archive_format}")
        os.replace(temporary_path, output)
    finally:
        temporary_path.unlink(missing_ok=True)


def verify_pair(first: Path, second: Path) -> str:
    first_hash = sha256_file(first)
    second_hash = sha256_file(second)
    if first_hash != second_hash:
        raise ValueError(
            f"reproducibility check failed: {first.name}={first_hash}, "
            f"{second.name}={second_hash}"
        )
    return first_hash


def validate_archive(artifact: Path, archive_format: str, runtime_path: str) -> None:
    if archive_format == "tar_gz":
        with tarfile.open(artifact, "r:gz") as archive:
            names = archive.getnames()
    elif archive_format == "zip":
        with zipfile.ZipFile(artifact) as archive:
            names = archive.namelist()
    else:
        raise ValueError(f"unsupported archive format: {archive_format}")

    normalized = {name.rstrip("/") for name in names}
    for name in normalized:
        path = PurePosixPath(name)
        if path.is_absolute() or ".." in path.parts:
            raise ValueError(f"unsafe path in {artifact.name}: {name}")
    if runtime_path not in normalized:
        raise ValueError(
            f"{artifact.name} does not contain the runtime at {runtime_path}"
        )


def provenance(
    spec: dict[str, Any],
    target_id: str,
    artifact: Path,
    toolchain: Path,
) -> dict[str, Any]:
    validate_spec(spec)
    target = next(
        (candidate for candidate in spec["targets"] if candidate["id"] == target_id),
        None,
    )
    if target is None:
        raise ValueError(f"unknown target: {target_id}")
    if artifact.name != target["artifact"]:
        raise ValueError(f"unexpected artifact name for {target_id}: {artifact.name}")
    return {
        "schema_version": 1,
        "artifact": {
            "name": artifact.name,
            "sha256": sha256_file(artifact),
            "size": artifact.stat().st_size,
        },
        "build": {
            "runner": target["runner"],
            "target": {
                "os": target["os"],
                "architecture": target["architecture"],
            },
            "reproducibility": {
                "independent_builds": 2,
                "archives_compared_by": "sha256",
                "source_date_epoch": spec["source"]["source_date_epoch"],
                "absolute_paths_remapped": True,
            },
            "cmake_defines": sorted(
                spec["runtime"]["common_cmake_defines"] + target["cmake_defines"]
            ),
            "toolchain": toolchain.read_text(encoding="utf-8").splitlines(),
        },
        "source": spec["source"],
    }


def promote_manifest(
    spec: dict[str, Any], manifest: dict[str, Any], artifacts_dir: Path
) -> dict[str, Any]:
    validate_spec(spec)
    manifest_targets = {
        (target["os"], target["architecture"]): target
        for target in manifest.get("targets", [])
    }
    if set(manifest_targets) != SUPPORTED_TARGETS:
        raise ValueError("Qore AI manifest does not contain exactly the six supported targets")
    if manifest_version_key(spec["runtime"]["manifest_version"]) <= manifest_version_key(
        manifest.get("version", "")
    ):
        raise ValueError("candidate manifest version must be newer than the current manifest")

    promoted = json.loads(json.dumps(manifest))
    promoted["version"] = spec["runtime"]["manifest_version"]
    promoted_targets = {
        (target["os"], target["architecture"]): target
        for target in promoted["targets"]
    }
    base_url = spec["runtime"]["release_base_url"].rstrip("/")
    for target in spec["targets"]:
        artifact = artifacts_dir / target["artifact"]
        provenance_path = artifacts_dir / f"{target['artifact']}.provenance.json"
        if not artifact.is_file() or not provenance_path.is_file():
            raise ValueError(f"missing artifact or provenance for {target['id']}")
        statement = read_json(provenance_path)
        actual_hash = sha256_file(artifact)
        actual_size = artifact.stat().st_size
        recorded = statement.get("artifact", {})
        recorded_target = statement.get("build", {}).get("target", {})
        if (
            recorded.get("name") != artifact.name
            or recorded.get("sha256") != actual_hash
            or recorded.get("size") != actual_size
            or recorded_target.get("os") != target["os"]
            or recorded_target.get("architecture") != target["architecture"]
            or statement.get("source") != spec["source"]
        ):
            raise ValueError(f"invalid provenance for {target['id']}")
        validate_archive(
            artifact, target["format"], target["runtime_relative_path"]
        )

        manifest_target = promoted_targets[(target["os"], target["architecture"])]
        if manifest_target["runtime_relative_path"] != target["runtime_relative_path"]:
            raise ValueError(f"runtime path changed for {target['id']}")
        manifest_target["runtime"].update(
            {
                "id": target["artifact"].removesuffix(".tar.gz").removesuffix(".zip"),
                "version": spec["runtime"]["version"],
                "url": f"{base_url}/{target['artifact']}",
                "size": actual_size,
                "sha256": actual_hash,
                "format": target["format"],
                "license": "MIT",
            }
        )
    return promoted


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    validate_parser = subparsers.add_parser("validate-spec")
    validate_parser.add_argument("--spec", type=Path, required=True)

    matrix_parser = subparsers.add_parser("matrix")
    matrix_parser.add_argument("--spec", type=Path, required=True)

    defines_parser = subparsers.add_parser("cmake-defines")
    defines_parser.add_argument("--spec", type=Path, required=True)
    defines_parser.add_argument("--target", required=True)

    package_parser = subparsers.add_parser("package")
    package_parser.add_argument("--source", type=Path, required=True)
    package_parser.add_argument("--output", type=Path, required=True)
    package_parser.add_argument("--format", choices=("tar_gz", "zip"), required=True)
    package_parser.add_argument("--root", default="")
    package_parser.add_argument("--source-date-epoch", type=int, required=True)

    verify_parser = subparsers.add_parser("verify-pair")
    verify_parser.add_argument("--first", type=Path, required=True)
    verify_parser.add_argument("--second", type=Path, required=True)

    provenance_parser = subparsers.add_parser("provenance")
    provenance_parser.add_argument("--spec", type=Path, required=True)
    provenance_parser.add_argument("--target", required=True)
    provenance_parser.add_argument("--artifact", type=Path, required=True)
    provenance_parser.add_argument("--toolchain", type=Path, required=True)
    provenance_parser.add_argument("--output", type=Path, required=True)

    promote_parser = subparsers.add_parser("promote-manifest")
    promote_parser.add_argument("--spec", type=Path, required=True)
    promote_parser.add_argument("--manifest", type=Path, required=True)
    promote_parser.add_argument("--artifacts-dir", type=Path, required=True)
    promote_parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if args.command == "validate-spec":
        validate_spec(read_json(args.spec))
    elif args.command == "matrix":
        print(json.dumps(matrix(read_json(args.spec)), separators=(",", ":")))
    elif args.command == "cmake-defines":
        spec = read_json(args.spec)
        validate_spec(spec)
        target = next(
            (
                candidate
                for candidate in spec["targets"]
                if candidate["id"] == args.target
            ),
            None,
        )
        if target is None:
            raise ValueError(f"unknown target: {args.target}")
        print(
            "\n".join(
                spec["runtime"]["common_cmake_defines"] + target["cmake_defines"]
            )
        )
    elif args.command == "package":
        package(
            args.source,
            args.output,
            args.format,
            args.root,
            args.source_date_epoch,
        )
        print(f"{sha256_file(args.output)}  {args.output.name}")
    elif args.command == "verify-pair":
        print(verify_pair(args.first, args.second))
    elif args.command == "provenance":
        write_json(
            args.output,
            provenance(
                read_json(args.spec),
                args.target,
                args.artifact,
                args.toolchain,
            ),
        )
    elif args.command == "promote-manifest":
        write_json(
            args.output,
            promote_manifest(
                read_json(args.spec),
                read_json(args.manifest),
                args.artifacts_dir,
            ),
        )


if __name__ == "__main__":
    main()
