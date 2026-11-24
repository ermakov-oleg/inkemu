"""Compare the official InkApp API contract with the lock in spec/contract.lock.

The lock keeps only section ids and hashes, never the contract text. See `--help`.
"""

from __future__ import annotations

import argparse
import difflib
import hashlib
import http.client
import json
import os
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path

UPSTREAM = "linchuanXu/xtapp-studio-mcp-intl"
INDEX_PATH = "knowledge/index.json"
LOCK_PATH = Path(__file__).resolve().parent.parent / "spec" / "contract.lock"
TIMEOUT = 30
SHA_RE = re.compile(r"[0-9a-f]{40}")


class SyncError(Exception):
    """Network or input failure: reported as one line, exit code 2."""


def section_id(source_path: str) -> str:
    """Derive the lock id of an index entry from its `sourcePath`."""
    for prefix, lang in (("contract/v2/spec-en/", "en"), ("contract/v2/spec/", "zh")):
        if source_path.startswith(prefix):
            stem = source_path[len(prefix) :].rsplit(".", 1)[0]
            m = re.match(r"(\d+)-", stem) or re.match(r"附录([A-Z])-", stem)
            if not m:
                raise SyncError(f"cannot derive a section id from {source_path}")
            return f"{lang}/{m.group(1)}"
    if source_path == "studio/preview.md":
        return "studio/preview"
    if "/" not in source_path:
        return f"guide/{source_path}"
    raise SyncError(f"cannot derive a section id from {source_path}")


def section_text(entry: dict) -> str:
    return entry["content"] + "\n\0\n" + (entry.get("example") or "")


def section_hash(entry: dict) -> str:
    return hashlib.sha256(section_text(entry).encode("utf-8")).hexdigest()


def sections(index: dict) -> dict[str, dict]:
    out: dict[str, dict] = {}
    for entry in index["entries"]:
        sid = section_id(entry["sourcePath"])
        if sid in out:
            raise SyncError(f"duplicate section id {sid}")
        out[sid] = entry
    return out


def build_lock(index: dict, commit: str) -> dict:
    return {
        "upstream": UPSTREAM,
        "commit": commit,
        "contract_tag": index["contractTag"],
        "schema_version": index["schemaVersion"],
        "generated_at": index["generatedAt"],
        "source_hash": index["sourceHash"],
        "sections": {sid: section_hash(e) for sid, e in sections(index).items()},
    }


def dump_lock(lock: dict) -> str:
    return json.dumps(lock, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def compare(lock: dict, index: dict) -> dict:
    new = {sid: section_hash(e) for sid, e in sections(index).items()}
    old = lock["sections"]
    return {
        "tag_old": lock["contract_tag"],
        "tag_new": index["contractTag"],
        "changed": sorted(s for s in old if s in new and old[s] != new[s]),
        "added": sorted(s for s in new if s not in old),
        "removed": sorted(s for s in old if s not in new),
    }


def _get(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "inkemu-spec-sync"})
    token = os.environ.get("GITHUB_TOKEN")
    if token and url.startswith("https://api.github.com/"):
        req.add_header("Authorization", f"Bearer {token}")
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
            return resp.read()
    except (urllib.error.URLError, TimeoutError, OSError, http.client.HTTPException) as exc:
        raise SyncError(f"network error fetching {url}: {exc}") from exc


def fetch_index(ref: str) -> dict:
    data = _get(f"https://raw.githubusercontent.com/{UPSTREAM}/{ref}/{INDEX_PATH}")
    try:
        return json.loads(data)
    except ValueError as exc:
        raise SyncError(f"upstream index at {ref} is not valid JSON") from exc


def latest_commit() -> str:
    data = _get(f"https://api.github.com/repos/{UPSTREAM}/commits?path={INDEX_PATH}&per_page=1")
    try:
        return json.loads(data)[0]["sha"]
    except (ValueError, LookupError, TypeError) as exc:
        raise SyncError("unexpected answer of the GitHub commits API") from exc


def load_json(path: str) -> dict:
    try:
        return json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise SyncError(f"cannot read {path}: {exc}") from exc


def check_schema(lock: dict, index: dict) -> None:
    if index.get("schemaVersion") != lock["schema_version"]:
        raise SyncError(f"upstream index schema changed: {lock['schema_version']} -> {index.get('schemaVersion')}")


def load_lock() -> dict:
    return load_json(str(LOCK_PATH))


def unified_diff(old_index: dict, new_index: dict, ids: list[str]) -> str:
    old, new = sections(old_index), sections(new_index)
    chunks = []
    for sid in ids:
        a = section_text(old[sid]).splitlines(keepends=True)
        b = section_text(new[sid]).splitlines(keepends=True)
        chunks.extend(difflib.unified_diff(a, b, f"{sid} (locked)", f"{sid} (upstream)"))
    return "".join(chunks)


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("command", nargs="?", choices=["status", "update"], default="status")
    p.add_argument("--ref", default="main", help="upstream git ref (default: main)")
    p.add_argument("--diff", action="store_true", help="status: also print the text diff of changed sections")
    p.add_argument("--check", action="store_true", help="exit 1 on any drift; print only ids and tags")
    p.add_argument("--json", action="store_true", help="status: machine-readable result")
    p.add_argument("--index", help="use this local index file instead of fetching the new one")
    p.add_argument("--old-index", help="use this local index file instead of fetching the locked commit (--diff)")
    p.add_argument("--commit", help="commit sha to record or report when --index is used")
    args = p.parse_args(argv)
    if args.check and args.diff:
        p.error("--check never prints contract text; do not combine it with --diff")
    if args.json and args.diff:
        p.error("--json cannot carry a text diff; use --diff alone")

    try:
        return run(args)
    except SyncError as exc:
        print(f"spec_sync: {exc}", file=sys.stderr)
        return 2
    except (KeyError, TypeError, AttributeError, ValueError, http.client.HTTPException) as exc:
        print(f"spec_sync: unexpected upstream index shape or answer ({type(exc).__name__}: {exc})", file=sys.stderr)
        return 2


def run(args: argparse.Namespace) -> int:
    lock = load_lock() if args.command == "status" else None

    if args.index:
        index = load_json(args.index)
        commit = args.commit or "unknown"
    elif args.check:
        index, commit = fetch_index(args.ref), "unknown"
    else:
        commit = latest_commit() if args.ref == "main" else args.ref
        index = fetch_index(commit)

    if args.command == "update":
        if not SHA_RE.fullmatch(commit):
            raise SyncError(f"update needs a 40-hex commit sha, got {commit!r}; pass --commit or a sha as --ref")
        LOCK_PATH.parent.mkdir(parents=True, exist_ok=True)
        new_lock = build_lock(index, commit)
        LOCK_PATH.write_text(dump_lock(new_lock), encoding="utf-8")
        print(f"lock updated: {new_lock['contract_tag']} at {commit}, {len(new_lock['sections'])} sections")
        return 0

    check_schema(lock, index)
    result = compare(lock, index)
    result["commit_old"] = lock["commit"]
    result["commit_new"] = commit
    drift = result["tag_old"] != result["tag_new"] or any(result[k] for k in ("changed", "added", "removed"))

    if args.json:
        print(json.dumps(result, indent=2, sort_keys=True))
    else:
        print(f"tag: {result['tag_old']} -> {result['tag_new']}")
        print(f"commit: {result['commit_old']} -> {result['commit_new']}")
        for key in ("changed", "added", "removed"):
            print(f"{key}: {', '.join(result[key]) or '-'}")
        if args.check:
            print("drift" if drift else "ok")

    if args.diff and result["changed"]:
        old_index = load_json(args.old_index) if args.old_index else fetch_index(lock["commit"])
        print(unified_diff(old_index, index, result["changed"]), end="")

    return 1 if args.check and drift else 0


if __name__ == "__main__":
    sys.exit(main())
