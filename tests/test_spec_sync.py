"""Tests of tools/spec_sync.py on synthetic indexes (never real contract text)."""

import importlib.util
import json
from pathlib import Path

import pytest

_spec = importlib.util.spec_from_file_location(
    "spec_sync", Path(__file__).resolve().parent.parent / "tools" / "spec_sync.py"
)
spec_sync = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(spec_sync)

SHA = "a" * 40


def entry(path, content="alpha", example=None):
    return {
        "topic": "t",
        "api": "a",
        "version": "1",
        "source": "s",
        "sourcePath": path,
        "content": content,
        "example": example,
    }


def make_index(entries=None, tag="api-1.0.0"):
    if entries is None:
        entries = [
            entry("contract/v2/spec-en/00-intro.md", "one", "ex"),
            entry("contract/v2/spec-en/\u9644\u5f55A-pack.md", "two"),
            entry("contract/v2/spec/07-text.md", "three"),
            entry("guide.en.md", "four"),
            entry("studio/preview.md", "five"),
        ]
    return {
        "schemaVersion": 3,
        "generatedAt": "2026-01-01T00:00:00Z",
        "contractTag": tag,
        "sourceHash": "h",
        "entries": entries,
    }


def write(path, data):
    path.write_text(json.dumps(data), encoding="utf-8")
    return str(path)


@pytest.fixture
def env(tmp_path, monkeypatch):
    lock = tmp_path / "spec" / "contract.lock"
    monkeypatch.setattr(spec_sync, "LOCK_PATH", lock)
    return tmp_path, lock


def run(*argv):
    return spec_sync.main(list(argv))


def locked(env, index=None):
    tmp, lock = env
    assert run("update", "--index", write(tmp / "old.json", index or make_index()), "--commit", SHA) == 0
    return lock


def test_section_ids():
    ids = {
        "contract/v2/spec-en/00-intro.md": "en/00",
        "contract/v2/spec-en/34-apps.md": "en/34",
        "contract/v2/spec/03-screen.md": "zh/03",
        "contract/v2/spec-en/\u9644\u5f55D-setup.md": "en/D",
        "contract/v2/spec/\u9644\u5f55A-pack.md": "zh/A",
        "ui-system.en.md": "guide/ui-system.en.md",
        "studio/preview.md": "studio/preview",
    }
    for path, sid in ids.items():
        assert spec_sync.section_id(path) == sid
    with pytest.raises(spec_sync.SyncError):
        spec_sync.section_id("other/dir/file.md")


def test_hash_format():
    import hashlib

    assert spec_sync.section_hash(entry("x", "c", "e")) == hashlib.sha256(b"c\n\0\ne").hexdigest()
    assert spec_sync.section_hash(entry("x", "c", None)) == hashlib.sha256(b"c\n\0\n").hexdigest()
    assert spec_sync.section_hash(entry("x", "\u0436", "")) == hashlib.sha256("\u0436\n\0\n".encode()).hexdigest()


def test_update_writes_stable_lock(env):
    lock = locked(env)
    first = lock.read_text(encoding="utf-8")
    data = json.loads(first)
    assert first.endswith("}\n") and first == json.dumps(data, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    assert data["commit"] == SHA and data["contract_tag"] == "api-1.0.0" and data["schema_version"] == 3
    assert sorted(data["sections"]) == ["en/00", "en/A", "guide/guide.en.md", "studio/preview", "zh/07"]
    locked(env)
    assert lock.read_text(encoding="utf-8") == first


def test_check_passes_on_same_index(env, capsys):
    locked(env)
    assert run("--check", "--index", write(env[0] / "new.json", make_index())) == 0
    assert "ok" in capsys.readouterr().out


def test_check_fails_on_changes_without_leaking_text(env, capsys):
    locked(env)
    entries = make_index()["entries"]
    entries[0]["content"] = "SECRET-NEW-TEXT"
    entries.pop(2)
    entries.append(entry("contract/v2/spec-en/01-new.md", "SECRET-ADDED"))
    code = run("--check", "--index", write(env[0] / "new.json", make_index(entries)))
    out = capsys.readouterr().out
    assert code == 1
    assert "changed: en/00" in out and "added: en/01" in out and "removed: zh/07" in out
    assert "SECRET" not in out


def test_check_fails_on_tag_change(env, capsys):
    locked(env)
    assert run("--check", "--index", write(env[0] / "new.json", make_index(tag="api-1.0.1"))) == 1
    assert "api-1.0.0 -> api-1.0.1" in capsys.readouterr().out


def test_status_exit_zero_and_json_shape(env, capsys):
    locked(env)
    entries = make_index()["entries"]
    entries[1]["example"] = "now with example"
    capsys.readouterr()
    code = run(
        "--json", "--index", write(env[0] / "new.json", make_index(entries, tag="api-2.0.0")), "--commit", "b" * 40
    )
    assert code == 0
    assert json.loads(capsys.readouterr().out) == {
        "tag_old": "api-1.0.0",
        "tag_new": "api-2.0.0",
        "commit_old": SHA,
        "commit_new": "b" * 40,
        "changed": ["en/A"],
        "added": [],
        "removed": [],
    }


def test_diff_uses_old_index(env, capsys):
    locked(env)
    entries = make_index()["entries"]
    entries[3]["content"] = "four changed"
    new = write(env[0] / "new.json", make_index(entries))
    old = write(env[0] / "old2.json", make_index())
    assert run("--diff", "--index", new, "--old-index", old) == 0
    out = capsys.readouterr().out
    assert "-four" in out and "+four changed" in out


def test_missing_lock_and_bad_index_exit_two(env, capsys):
    assert run("--index", write(env[0] / "n.json", make_index())) == 2
    assert run("update", "--index", str(env[0] / "absent.json")) == 2
    assert "spec_sync:" in capsys.readouterr().err


def test_network_error_exit_two(env, monkeypatch, capsys):
    locked(env)

    def boom(url):
        raise spec_sync.SyncError("network error fetching x")

    monkeypatch.setattr(spec_sync, "_get", boom)
    assert run("--check") == 2
    assert capsys.readouterr().err.count("\n") == 1


def test_check_with_diff_is_a_usage_error(env, capsys):
    locked(env)
    with pytest.raises(SystemExit) as exc:
        run("--check", "--diff", "--index", write(env[0] / "new.json", make_index()))
    assert exc.value.code == 2
    assert "--diff" in capsys.readouterr().err


def test_malformed_index_exits_two(env, capsys):
    locked(env)
    bad = {"schemaVersion": 3, "contractTag": "t", "items": []}
    assert run("--check", "--index", write(env[0] / "bad.json", bad)) == 2
    null_content = make_index([entry("guide.en.md", None)])
    assert run("--check", "--index", write(env[0] / "null.json", null_content)) == 2
    err = capsys.readouterr().err
    assert err.count("\n") == 2 and err.count("spec_sync:") == 2


def test_schema_version_change_exits_two(env, capsys):
    locked(env)
    index = make_index()
    index["schemaVersion"] = 4
    assert run("--check", "--index", write(env[0] / "new.json", index)) == 2
    assert "schema changed: 3 -> 4" in capsys.readouterr().err


def test_incomplete_read_exits_two(env, monkeypatch, capsys):
    import http.client

    locked(env)

    class Resp:
        def __enter__(self):
            return self

        def __exit__(self, *exc):
            return False

        def read(self):
            raise http.client.IncompleteRead(b"x")

    monkeypatch.setattr(spec_sync.urllib.request, "urlopen", lambda *a, **k: Resp())
    assert run("--check") == 2
    assert capsys.readouterr().err.count("\n") == 1


def test_update_refuses_non_sha_commit(env, capsys):
    tmp, lock = env
    index = write(tmp / "i.json", make_index())
    assert run("update", "--index", index) == 2
    assert run("update", "--index", index, "--commit", "main") == 2
    assert run("update", "--ref", "some-branch", "--index", index, "--commit", "ABC") == 2
    assert not lock.exists()
    assert capsys.readouterr().err.count("40-hex") == 3


def test_diff_with_json_is_a_usage_error(env, capsys):
    locked(env)
    with pytest.raises(SystemExit) as exc:
        run("--json", "--diff", "--index", write(env[0] / "new.json", make_index()))
    assert exc.value.code == 2
    assert "--json" in capsys.readouterr().err
