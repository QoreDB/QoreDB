#!/usr/bin/env python3
# SPDX-License-Identifier: BUSL-1.1
"""Create disposable v0.1.39-format data; never read the user's QoreDB profile.

Schema reference: tag v0.1.39 (252e0b267b6d67585b6916bce2ce29f516685fd8),
vault/credentials.rs, workspace/types.rs, time_travel/types.rs, notebookTypes.ts
and query/queryLibrary.ts. Browser state is seeded separately in the native WebView.
The target must not already exist. No licence or credential is forged.
"""
import hashlib
import json
from pathlib import Path
import sqlite3
import sys


root = Path(sys.argv[1]).resolve()
root.mkdir(parents=True, exist_ok=False)
stamp = "2026-09-24T12:00:00Z"


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def project_id(path):
    value = 0xCBF29CE484222325
    for byte in str(path).encode():
        value = ((value ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"ws_{value:016x}"


for directory in ["config", "data", "cache", "runtime"]:
    (root / directory).mkdir(mode=0o700)
db_path = root / "fixture.sqlite"
with sqlite3.connect(db_path) as db:
    db.execute("CREATE TABLE items(id INTEGER PRIMARY KEY, label TEXT NOT NULL, email TEXT, amount INTEGER)")
    db.executemany("INSERT INTO items VALUES (?, ?, ?, ?)", [
        (1, "Avant mise à jour", "synthetic@example.invalid", 9007199254740993),
        (2, "Deuxième", "other@example.invalid", 42),
    ])

library = {"version": 1, "folders": [{"id": "legacy-folder", "name": "Qualification", "createdAt": 1, "updatedAt": 1}],
           "items": [{"id": "legacy-query", "title": "Requête conservée", "query": "SELECT * FROM items WHERE id = {{id}}", "folderId": "legacy-folder", "tags": ["qualification"], "isFavorite": True, "driver": "sqlite", "variables": {"id": {"name": "id", "type": "number", "defaultValue": "1"}}, "createdAt": 1, "updatedAt": 1}]}
notebook = {"version": 1, "metadata": {"id": "legacy-notebook", "title": "Carnet conservé", "createdAt": stamp, "updatedAt": stamp, "tags": ["qualification"]},
            "cells": [{"id": "legacy-markdown", "type": "markdown", "source": "# Données synthétiques v0.1.39", "executionState": "idle"},
                      {"id": "legacy-sql", "type": "sql", "source": "SELECT id, label, amount FROM items", "executionState": "idle", "config": {"label": "items", "maxRows": 10, "collapsed": False}}],
            "variables": {"id": {"name": "id", "type": "number", "defaultValue": "9007199254740993"}}}
connections = []
workspaces = []
for name in ["default", "alpha", "beta"]:
    directory = root / "config/com.rapha.qoredb" if name == "default" else root / name / ".qoredb"
    pid = "default" if name == "default" else project_id(directory)
    connection = {"id": f"legacy-{name}", "name": f"SQLite {name} v0139", "driver": "sqlite", "environment": "development", "read_only": False, "expose_to_agents": False,
                  "host": str(db_path), "port": 0, "username": "", "database": None, "ssl": False, "ssh_tunnel": None, "project_id": pid,
                  "masking": {"rules": [{"table": "items", "column": "email", "mode": "hidden"}], "mask_detected_columns": False}}
    if name == "default":
        write(directory / "connections.json", [connection])
    else:
        write(directory / "connections" / f"legacy-{name}.json", connection)
        write(directory / "workspace.json", {"version": 1, "name": name, "created_at": stamp, "updated_at": stamp})
        workspaces.append({"path": str(directory), "name": name, "last_opened": stamp})
    write(directory / "queries/library.json", library)
    write(directory / "notebooks/legacy.qnb", notebook)
    connections.append(connection)
write(root / "config/com.rapha.qoredb/recent_workspaces.json", workspaces)
history_dir = root / "data/com.qoredb.app/time-travel"
write(history_dir / "time-travel.json", {"enabled": False, "max_entries": 12345, "retention_days": 0, "max_file_size_mb": 0, "excluded_tables": ["sessions"], "production_only": True, "sensitive_columns": ["email"]})
event = {"id": "c15477e3-87ef-41f9-9b1e-2ba24d73e381", "timestamp": stamp, "session_id": "legacy-session", "driver_id": "sqlite", "namespace": {"database": "main", "schema": None}, "table_name": "items", "operation": "insert", "primary_key": {"id": 9007199254740993}, "before": None, "after": {"id": 9007199254740993, "email": "[REDACTED]"}, "changed_columns": [], "connection_name": "SQLite default v0139", "environment": "development"}
(history_dir / "changelog.jsonl").write_text(json.dumps(event) + "\n")
snapshot_id = "1325d1c7-7d46-4c1a-a4c9-9bd62ab0ca57"
write(root / "data/com.qoredb.app/snapshots" / f"{snapshot_id}.json", {
    "meta": {"id": snapshot_id, "name": "Capture v0139", "description": "Synthetic",
             "source": "SELECT amount FROM items", "source_type": "query",
             "connection_name": "SQLite default v0139", "driver": "sqlite",
             "namespace": {"database": "main", "schema": None},
             "columns": [{"name": "amount", "data_type": "INTEGER", "nullable": False}],
             "row_count": 1, "created_at": stamp, "file_size": 0},
    "rows": [{"values": [{"$qoreInt": "9007199254740993"}]}],
})
browser = {"i18nextLng": "fr", "qoredb-theme": "dark", "qoredb_diagnostics_settings": json.dumps({"storeHistory": True, "storeErrorLogs": False}),
           "qoredb_query_history": json.dumps([{"id": "legacy-history", "query": "SELECT 9007199254740993", "sessionId": "legacy-session", "driver": "sqlite", "executedAt": 1790251200000, "rowCount": 1}]),
           "qoredb_query_library_v1": json.dumps({"folders": library["folders"], "items": library["items"]}),
           "qnb_draft_legacy": json.dumps(notebook)}
write(root / "browser-seed.json", browser)
write(root / "fixture-manifest.json", {"baseline": "252e0b267b6d67585b6916bce2ce29f516685fd8", "connections": connections, "workspaces": workspaces})
hashes = {str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest() for path in root.rglob("*") if path.is_file()}
write(root / "before-sha256.json", hashes)
print(root)
