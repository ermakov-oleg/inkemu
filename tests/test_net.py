import hashlib
import json

import pytest

DRAW = "msg = 'none'\nfunction on_draw(c, g) g:clear(0); g:text(0, 0, 'msg ' .. tostring(msg)) end\n"
NET = {"permissions": ["net"]}


def app_with(make_app, request_lua: str, cb_body: str = "msg = resp.status .. ':' .. tostring(resp.body)", **kw):
    lua = (
        DRAW
        + "function on_load(c)\n  job = c.net:request("
        + request_lua
        + ", function(c, resp)\n"
        + cb_body
        + "\n    c:invalidate()\n  end)\nend\n"
    )
    return make_app(lua, manifest=NET, **kw)


def write_fixture(tmp_path, name, req, resp_extra, body="ok", match="exact"):
    d = tmp_path / "fixtures" / "net"
    d.mkdir(parents=True, exist_ok=True)
    doc = {"request": req, "match": match, "response": {"headers": {}, "body": body, **resp_extra}}
    (d / f"{name}.json").write_text(json.dumps(doc), encoding="utf-8")


def test_replay_hit(make_app, emu_factory, tmp_path):
    write_fixture(tmp_path, "hand", {"method": "GET", "url": "http://x.test/a"}, {"status": 200})
    emu = emu_factory(app_with(make_app, "{url = 'http://x.test/a'}"))
    emu.wait(299)
    assert emu.has_text("msg none")
    emu.wait(1)
    assert emu.has_text("msg 200:ok")


def test_response_fields_and_delay(make_app, emu_factory, tmp_path):
    write_fixture(
        tmp_path,
        "h",
        {"method": "GET", "url": "http://x.test/a"},
        {"status": 201, "delay_ms": 1000, "headers": {"X-Foo": "bar"}},
        body="abc",
    )
    emu = emu_factory(
        app_with(
            make_app,
            "{url = 'http://x.test/a', method = 'GET'}",
            "msg = resp.code .. resp.headers['x-foo'] .. resp.bytes",
        )
    )
    emu.wait(999)
    assert emu.has_text("msg none")
    emu.wait(1)
    assert emu.has_text("msg 201bar3")


def test_replay_without_fixture(make_app, emu_factory):
    emu = emu_factory(app_with(make_app, "{url = 'http://x.test/b'}", "msg = resp.err .. resp.error .. resp.status"))
    emu.wait(300)
    assert emu.has_text("msg no_fixtureno_fixture0")
    line = next(ln for ln in emu.log if "no fixture" in ln)
    assert "GET http://x.test/b" in line and "--net record" in line


def test_record_then_replay(make_app, emu_factory, tmp_path, server):
    url = server.base + "/data"
    emu = emu_factory(
        app_with(
            make_app, "{url = '%s'}" % url, "msg = resp.status .. ':' .. #resp.body .. resp.headers['content-type']"
        ),
        net="record",
    )
    emu.wait(300)
    assert emu.has_text("msg 200:5text/plain")
    files = sorted(p.name for p in (tmp_path / "fixtures" / "net").iterdir())
    assert len(files) == 2 and files[0].startswith("GET_127.0.0.1_") and files[1].endswith(".json")
    assert (tmp_path / "fixtures" / "net" / files[0]).read_bytes() == b"\x00\xff\xfeok"
    assert "set-cookie" not in (tmp_path / "fixtures" / "net" / files[1]).read_text()
    server.shutdown()
    emu2 = emu_factory(
        app_with(make_app, "{url = '%s'}" % url, "msg = resp.status .. ':' .. #resp.body"), state_dir=tmp_path / "s2"
    )
    emu2.wait(300)
    assert emu2.has_text("msg 200:5")


def test_binary_body_reaches_lua(make_app, emu_factory, tmp_path, server):
    emu = emu_factory(
        app_with(
            make_app,
            "{url = '%s/x'}" % server.base,
            "msg = resp.body:byte(1) .. ',' .. resp.body:byte(2) .. ',' .. resp.body:byte(3)",
        ),
        net="live",
    )
    emu.wait(300)
    assert emu.has_text("msg 0,255,254")


def test_record_redacts_secret_in_path(make_app, emu_factory, tmp_path, server):
    secrets = tmp_path / "secrets.json"
    secrets.write_text(json.dumps({"cal": "private-abc123"}))
    emu = emu_factory(
        app_with(make_app, "{url = '%s/ical/private-abc123/basic.ics'}" % server.base),
        net="record",
        secrets_file=secrets,
    )
    emu.wait(300)
    d = tmp_path / "fixtures" / "net"
    assert d.is_dir()
    for p in d.iterdir():
        assert "abc123" not in p.name and "abc123" not in p.read_bytes().decode("latin-1")
    assert "{{cal}}" in next(d.glob("*.json")).read_text()
    assert not any("abc123" in ln for ln in emu.log + emu.warnings)


def test_record_drops_auth_headers(make_app, emu_factory, tmp_path, server):
    emu = emu_factory(
        app_with(
            make_app,
            "{url = '%s/h', headers = {Authorization = 'Bearer s3cr3t', ['X-Api-Key'] = 'k1', "
            "['User-Agent'] = 'inkemu', ['X-My-Token'] = 't'}}" % server.base,
            "msg = resp.status",
        ),
        net="record",
    )
    emu.wait(300)
    text = next((tmp_path / "fixtures" / "net").glob("*.json")).read_text()
    assert "s3cr3t" not in text and "uthorization" not in text and "pi-Key" not in text and "Token" not in text
    assert "inkemu" in text
    assert server.hits[0][1]["Authorization"] == "Bearer s3cr3t"


def test_placeholder_url_matches(make_app, emu_factory, tmp_path, server):
    secrets = tmp_path / "secrets.json"
    secrets.write_text(json.dumps({"cal": "private-abc123"}))
    emu = emu_factory(
        app_with(make_app, "{url = '%s/ical/private-abc123/basic.ics'}" % server.base),
        net="record",
        secrets_file=secrets,
    )
    emu.wait(300)
    server.shutdown()
    # literal placeholder, no secrets known: matched as is
    emu2 = emu_factory(
        app_with(make_app, "{url = '%s/ical/{{cal}}/basic.ics'}" % server.base, "msg = resp.status"),
        state_dir=tmp_path / "s2",
        secrets_file=tmp_path / "none.json",
    )
    emu2.wait(300)
    assert emu2.has_text("msg 200")


def test_placeholder_expands_for_real_request(make_app, emu_factory, tmp_path, server):
    secrets = tmp_path / "secrets.json"
    secrets.write_text(json.dumps({"cal": "private-abc123"}))
    emu = emu_factory(
        app_with(make_app, "{url = '%s/ical/{{cal}}/basic.ics'}" % server.base), net="live", secrets_file=secrets
    )
    emu.wait(300)
    assert server.hits[0][0] == "/ical/private-abc123/basic.ics"
    assert not any("abc123" in ln for ln in emu.log)


def test_unknown_secret_warns(make_app, emu_factory, server):
    emu = emu_factory(
        app_with(make_app, "{url = '%s/ical/abcdefghij0123456789xyz/basic.ics'}" % server.base), net="record"
    )
    assert any("looks like a secret" in w and "abcdefghij0123456789xyz" not in w for w in emu.warnings)


def test_patch_never_answers(make_app, emu_factory):
    emu = emu_factory(app_with(make_app, "{url = 'http://x.test/p', method = 'PATCH'}"))
    emu.wait("60s")
    assert emu.has_text("msg none")
    assert any("PATCH" in w for w in emu.warnings)


def test_glob_match_fixture(make_app, emu_factory, tmp_path):
    write_fixture(tmp_path, "err", {"method": "GET", "url": "http://x.test/err*"}, {"status": 500}, match="glob")
    emu = emu_factory(app_with(make_app, "{url = 'http://x.test/errors/1'}"))
    emu.wait(300)
    assert emu.has_text("msg 500:")


def test_prefix_match_fixture_and_exact_wins(make_app, emu_factory, tmp_path):
    write_fixture(tmp_path, "a", {"method": "GET", "url": "http://x.test/"}, {"status": 404}, match="prefix")
    write_fixture(tmp_path, "b", {"method": "GET", "url": "http://x.test/a"}, {"status": 200})
    emu = emu_factory(app_with(make_app, "{url = 'http://x.test/a'}"))
    emu.wait(300)
    assert emu.has_text("msg 200:")


def test_post_matches_by_body_hash(make_app, emu_factory, tmp_path):
    sha = hashlib.sha256(b"q=1").hexdigest()
    write_fixture(
        tmp_path, "p", {"method": "POST", "url": "http://x.test/p", "body_sha256": sha}, {"status": 200}, body="yes"
    )
    emu = emu_factory(app_with(make_app, "{url = 'http://x.test/p', method = 'POST', body = 'q=1'}"))
    emu.wait(300)
    assert emu.has_text("msg 200:yes")
    emu2 = emu_factory(
        app_with(make_app, "{url = 'http://x.test/p', method = 'POST', body = 'q=2'}"), state_dir=tmp_path / "s2"
    )
    emu2.wait(300)
    assert emu2.has_text("msg 0:nil")


def test_cancel_before_delivery(make_app, emu_factory, tmp_path):
    write_fixture(tmp_path, "h", {"method": "GET", "url": "http://x.test/a"}, {"status": 200})
    emu = emu_factory(app_with(make_app, "{url = 'http://x.test/a'}"))
    emu.wait(100)
    emu.lua_exec("job:cancel()")
    emu.wait(1000)
    assert emu.has_text("msg none")


def test_net_nil_without_permission(make_app, emu_factory):
    emu = emu_factory(make_app(DRAW + "function on_load(c) msg = tostring(c.net) end"))
    assert emu.has_text("msg nil")


@pytest.mark.allow_errors
def test_keep_wifi_and_websocket(make_app, emu_factory):
    emu = emu_factory(
        make_app(
            DRAW + "function on_load(c) c.net:keep_wifi(300000) msg = 'ok'\n"
            "local ok, e = pcall(function() c.net:websocket({}) end) msg = msg .. tostring(e)\n"
            "c.net:keep_wifi(true) end",
            manifest=NET,
        )
    )
    assert any("bad argument" in e for e in emu.errors)
    assert "not emulated" in emu.lua_eval("return msg")


def test_live_mode_hits_server(make_app, emu_factory, tmp_path, server):
    emu = emu_factory(app_with(make_app, "{url = '%s/live'}" % server.base, "msg = resp.status"), net="live")
    emu.wait(300)
    assert emu.has_text("msg 200") and server.hits
    assert not (tmp_path / "fixtures").exists()


def test_network_error_gives_err(make_app, emu_factory):
    emu = emu_factory(
        app_with(
            make_app,
            "{url = 'http://127.0.0.1:1/x', timeout_ms = 2000}",
            "msg = resp.status .. tostring(resp.err) .. tostring(resp.body)",
        ),
        net="live",
    )
    emu.wait(300)
    assert emu.has_text("msg 0no_networknil")


@pytest.mark.allow_errors
def test_request_argument_errors(make_app, emu_factory):
    emu = emu_factory(
        make_app(
            DRAW + "function on_load(c)\n local a, e = c.net:request({url = 'nonsense'}, function() end)\n"
            "msg = tostring(a) .. tostring(e)\n c.net:request({}, function() end) end",
            manifest=NET,
        )
    )
    assert "bad extra argument" in emu.errors[0]
    assert emu.lua_eval("return msg") == "nilinvalid_argument"


def test_hyphenated_name_and_numeric_value_are_redacted(make_app, emu_factory, tmp_path, server):
    secrets = tmp_path / "secrets.json"
    secrets.write_text(json.dumps({"my-cal": "private-abc123", "pin": 987654}))
    emu = emu_factory(
        app_with(make_app, "{url = '%s/ical/private-abc123/987654/basic.ics'}" % server.base),
        net="record",
        secrets_file=secrets,
    )
    emu.wait(300)
    d = tmp_path / "fixtures" / "net"
    for p in d.iterdir():
        text = p.name + p.read_bytes().decode("latin-1")
        assert "abc123" not in text and "987654" not in text
    assert "{{my-cal}}/{{pin}}" in next(d.glob("*.json")).read_text()
    assert emu.warnings == []


@pytest.mark.allow_errors
@pytest.mark.parametrize(
    "content", ['{"bad name": "private-abc123"}', '{"cal": ["private-abc123"]}', "{not json private-abc123"]
)
def test_unusable_secrets_refuse_record(make_app, emu_factory, tmp_path, server, content):
    secrets = tmp_path / "secrets.json"
    secrets.write_text(content)
    emu = emu_factory(
        app_with(make_app, "{url = '%s/ical/private-abc123/b'}" % server.base, "msg = resp.err"),
        net="record",
        secrets_file=secrets,
    )
    emu.wait(300)
    assert emu.has_text("msg secrets_invalid") and not server.hits
    assert not (tmp_path / "fixtures").exists()
    assert any("recording refused" in e for e in emu.errors)
    assert emu.warnings and not any("abc123" in ln for ln in emu.log + emu.warnings + emu.errors)


def test_unusable_secret_warns_in_replay(make_app, emu_factory, tmp_path):
    secrets = tmp_path / "secrets.json"
    secrets.write_text('{"bad name": "x", "ok": "y"}')
    emu = emu_factory(app_with(make_app, "{url = 'http://x.test/a'}"), secrets_file=secrets)
    assert any("'bad name'" in w for w in emu.warnings) and emu.errors == []


def two_requests(url_lua: str, body_lua: str) -> str:
    """A GET of `url_lua` and a POST of `body_lua` (Lua expressions); `msg` collects status and body of each."""
    cb = "function(c, r) msg = msg .. r.status .. tostring(r.body); c:invalidate() end"
    return (
        DRAW + "function on_load(c)\n  msg = ''\n"
        f"  c.net:request({{url = {url_lua}}}, {cb})\n"
        f"  c.net:request({{url = 'http://h.test/p', method = 'POST', body = {body_lua}}}, {cb})\nend\n"
    )


def fixtures_for(tmp_path, url: str, body: bytes) -> None:
    """Fixtures that answer only the redacted forms: `url` and a POST body hashing like `body`."""
    write_fixture(tmp_path, "u", {"method": "GET", "url": url}, {"status": 200}, body="url")
    sha = hashlib.sha256(body).hexdigest()
    write_fixture(
        tmp_path, "b", {"method": "POST", "url": "http://h.test/p", "body_sha256": sha}, {"status": 200}, body="body"
    )


@pytest.mark.parametrize("shown", ["pa+ss%2Fwd%2Bx", "pa%20ss%2fwd%2bx", "pa%20ss%2Fwd%2Bx", "pa ss/wd+x"])
def test_redact_quote_plus_and_lowercase_percent(make_app, emu_factory, tmp_path, shown):
    secrets = tmp_path / "secrets.json"
    secrets.write_text(json.dumps({"pw": "pa ss/wd+x"}))
    fixtures_for(tmp_path, "http://h.test/a/{{pw}}/b", b"x={{pw}};")
    emu = emu_factory(
        make_app(two_requests(f"'http://h.test/a/{shown}/b'", f"'x={shown};'"), manifest=NET), secrets_file=secrets
    )
    emu.wait(300)
    assert emu.lua_eval("return msg") == "200url200body"
    assert any("GET http://h.test/a/{{pw}}/b -> 200 (fixture)" in ln for ln in emu.log)


def test_userinfo_password_is_always_redacted(make_app, emu_factory, tmp_path):
    write_fixture(tmp_path, "u", {"method": "GET", "url": "https://bob:{{userinfo}}@h.test/x?q=1"}, {"status": 200})
    lua = (
        DRAW + "function on_load(c)\n  msg = ''\n"
        "  for _, u in ipairs({'https://bob:hunter2@h.test/x?q=1', 'https://bob@h.test/x'}) do\n"
        "    c.net:request({url = u}, function(c, r) msg = msg .. r.status .. ',' end)\n  end\nend\n"
    )
    emu = emu_factory(make_app(lua, manifest=NET))
    emu.wait(300)
    assert emu.lua_eval("return msg") == "200,0,"
    assert any("GET https://bob:{{userinfo}}@h.test/x?q=1 -> 200 (fixture)" in ln for ln in emu.log)
    assert any("no fixture for GET https://bob@h.test/x;" in w for w in emu.warnings)
    assert not any("hunter2" in ln for ln in emu.log + emu.warnings)


def test_record_redacts_encoded_forms_everywhere(make_app, emu_factory, tmp_path, server):
    secrets = tmp_path / "secrets.json"
    secrets.write_text(json.dumps({"pw": "pa ss/wd+x"}))
    lua = (
        DRAW + "function on_load(c)\n"
        f"  c.net:request({{url = '{server.base}/echo/pa+ss%2Fwd%2Bx'}}, function() end)\n"
        f"  c.net:request({{url = '{server.base}/echo2/pa%20ss%2fwd%2bx'}}, function() end)\nend\n"
    )
    emu = emu_factory(make_app(lua, manifest=NET), net="record", secrets_file=secrets)
    emu.wait(300)
    d = tmp_path / "fixtures" / "net"
    assert len(list(d.glob("*.json"))) == 2
    for text in [p.read_bytes().decode("latin-1") for p in d.iterdir()] + emu.log + emu.warnings:
        low = text.lower()
        assert "pa+ss" not in low and "pa%20ss" not in low and "wd%2b" not in low, text


@pytest.mark.parametrize(
    "text",
    [
        '{"site":"https://example.com","email":"a@b.c"}',
        'https://h:8080","m":"x@y',
        "<a href='https://h.test:1/'>x@y</a>",
    ],
)
def test_userinfo_redaction_leaves_bodies_alone(make_app, emu_factory, tmp_path, text):
    fixtures_for(tmp_path, "http://h.test/?q=" + text, text.encode())
    emu = emu_factory(make_app(two_requests(f"'http://h.test/?q=' .. [==[{text}]==]", f"[==[{text}]==]"), manifest=NET))
    emu.wait(300)
    assert emu.lua_eval("return msg") == "200url200body"


def test_userinfo_password_not_in_log_or_warnings(make_app, emu_factory, server):
    url = server.base.replace("http://", "http://bob:hunter2@") + "/ical/abcdefghij0123456789xyz/basic.ics"
    emu = emu_factory(app_with(make_app, "{url = '%s'}" % url, "msg = resp.status"), net="record")
    emu.wait(300)
    assert any("looks like a secret" in w for w in emu.warnings)
    assert not any("hunter2" in ln for ln in emu.log + emu.warnings)


@pytest.mark.parametrize(
    "bad",
    [
        {"delay_ms": None},
        {"headers": ["a", "b"]},
        {"status": "ok"},
        {"delay_ms": -1},
        {"body_file": 5},
        {"status": True},
    ],
)
def test_malformed_fixture_is_ignored_with_a_warning(make_app, emu_factory, tmp_path, bad):
    write_fixture(tmp_path, "hand", {"method": "GET", "url": "http://x.test/a"}, bad)
    emu = emu_factory(app_with(make_app, "{url = 'http://x.test/a'}", "msg = tostring(resp.err)"))
    emu.wait(300)
    assert emu.has_text("msg no_fixture")
    assert any("hand.json is not valid" in w for w in emu.warnings), emu.warnings


def test_missing_body_file_warns_and_gives_empty_body(make_app, emu_factory, tmp_path):
    write_fixture(
        tmp_path, "hand", {"method": "GET", "url": "http://x.test/a"}, {"status": 201, "body_file": "gone.body"}
    )
    emu = emu_factory(app_with(make_app, "{url = 'http://x.test/a'}", "msg = resp.status .. ':' .. #resp.body"))
    emu.wait(300)
    assert emu.has_text("msg 201:0")
    assert any("gone.body" in w and "not found" in w for w in emu.warnings), emu.warnings


def test_truncated_response_is_no_network(make_app, emu_factory, server):
    emu = emu_factory(app_with(make_app, "{url = '%s/short'}" % server.base, "msg = tostring(resp.err)"), net="live")
    emu.wait(300)
    assert emu.has_text("msg no_network")


@pytest.mark.allow_errors
def test_secrets_are_read_again_on_reload_and_restart(make_app, emu_factory, tmp_path, server):
    secrets = tmp_path / "secrets.json"
    secrets.write_text("{}")
    write_fixture(tmp_path, "pin", {"method": "GET", "url": "http://x.test/{{pin}}"}, {"status": 200})
    emu = emu_factory(app_with(make_app, "{url = 'http://x.test/abcdef'}", "msg = resp.status"), secrets_file=secrets)
    emu.wait(300)
    assert emu.has_text("msg 0")
    secrets.write_text(json.dumps({"pin": "abcdef"}))
    emu.reload()
    emu.wait(300)
    assert emu.has_text("msg 200")
    assert any("GET http://x.test/{{pin}} -> 200 (fixture)" in ln for ln in emu.log)
    secrets.write_text('{"bad name": "x"}')
    emu.restart()
    assert any("'bad name'" in w for w in emu.warnings)
    secrets.write_text("{}")
    emu3 = emu_factory(
        app_with(make_app, "{url = '%s/a'}" % server.base, "msg = resp.err"), net="record", secrets_file=secrets
    )
    assert not emu3.errors
    secrets.write_text('{"bad name": "x"}')
    emu3.reload()
    emu3.wait(300)
    assert emu3.has_text("msg secrets_invalid")
    assert any("recording refused" in e for e in emu3.errors)


def test_secret_added_before_reload_is_not_recorded(make_app, emu_factory, tmp_path, server):
    secrets = tmp_path / "secrets.json"
    secrets.write_text("{}")
    emu = emu_factory(
        app_with(make_app, "{url = '%s/echo/tok-98765'}" % server.base, "msg = resp.status"),
        net="record",
        secrets_file=secrets,
    )
    emu.wait(300)
    d = tmp_path / "fixtures" / "net"
    assert any("tok-98765" in p.read_text() for p in d.iterdir())
    for p in d.iterdir():
        p.unlink()
    secrets.write_text(json.dumps({"tok": "tok-98765"}))
    emu.reload()
    emu.wait(300)
    assert emu.has_text("msg 200") and len(list(d.glob("*.json"))) == 1
    for p in d.iterdir():
        assert "tok-98765" not in p.read_text(), p
    assert "{{tok}}" in next(d.glob("*.json")).read_text()


def test_live_request_in_a_timer_fires_at_request_time_plus_delay(make_app, emu_factory, server):
    lua = (
        DRAW + "function on_load(c)\n"
        "  c:after(1000, function()\n"
        f"    c.net:request({{url = '{server.base}/a'}}, function(c, r) at = c.clock:millis() end)\n"
        "  end)\nend\n"
    )
    emu = emu_factory(make_app(lua, manifest=NET), net="live")
    emu.wait(5000)
    assert emu.lua_eval("return at") == 1300


def test_big_integer_secret_is_redacted_by_its_digits(make_app, emu_factory, tmp_path):
    secrets = tmp_path / "secrets.json"
    secrets.write_text('{"pin": 123456789012345678901234}')
    write_fixture(tmp_path, "pin", {"method": "GET", "url": "http://x.test/{{pin}}"}, {"status": 200})
    emu = emu_factory(
        app_with(make_app, "{url = 'http://x.test/123456789012345678901234'}", "msg = resp.status"),
        secrets_file=secrets,
    )
    emu.wait(300)
    assert emu.has_text("msg 200")
    assert not any("123456789012345678901234" in ln for ln in emu.log)
