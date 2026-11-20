import fnmatch
import hashlib
import http.client
import json
import re
import socket
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Callable
from urllib.parse import quote, quote_plus, urlsplit

from .ctx_basic import check_arg, raw
from .lua_env import make_object

if TYPE_CHECKING:
    from .emu import Emu

DEFAULT_DELAY_MS = 300
DEFAULT_TIMEOUT_MS = 30_000
SENSITIVE_HEADER = re.compile(r"^(authorization|proxy-authorization|cookie|set-cookie|x-api-key)$|token|key|secret", re.I)
_NAME = r"[A-Za-z0-9_.-]+"
_PLACEHOLDER = re.compile(r"\{\{(" + _NAME + r")\}\}")
_SECRET_LIKE = re.compile(r"[A-Za-z0-9_-]{20,}")
_URL = re.compile(r"https?://[^/\s?#]+", re.I)
# quotes, commas and brackets end a URL in JSON/HTML bodies, so they may not belong to the userinfo
_USERINFO_PASSWORD = re.compile(r"(\w+://[^/\s:@?#\"',<>`]*):[^/\s@?#\"',<>`]*@")
_USERINFO_PASSWORD_B = re.compile(_USERINFO_PASSWORD.pattern.encode())

# Escapes bytes >= 0x80 and `\` so lupa never has to decode a non-UTF-8 string from the app.
_OPTS = r"""
local gsub, type, fmt = string.gsub, type, string.format
local function esc(s) return (gsub(s, '[\\\128-\255]', function(c) return fmt('\\x%02x', c:byte()) end)) end
return function(o)
  local out = { headers = {} }
  for _, k in ipairs({ 'url', 'method', 'body' }) do
    local v = o[k]
    if type(v) == 'string' then out[k] = esc(v) elseif v ~= nil then out[k .. '_type'] = type(v) end
  end
  out.timeout_ms = type(o.timeout_ms) == 'number' and o.timeout_ms or nil
  if type(o.headers) == 'table' then
    for k, v in pairs(o.headers) do
      if type(k) == 'string' and (type(v) == 'string' or type(v) == 'number') then out.headers[esc(k)] = esc(tostring(v)) end
    end
  end
  return out
end
"""


@dataclass
class Response:
    status: int
    headers: dict[str, str] = field(default_factory=dict)
    body: bytes | None = None
    err: str | None = None
    delay_ms: int = DEFAULT_DELAY_MS


def _sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


class Net:
    def __init__(self, mode: str, fixtures_dir: Path, secrets: dict[str, str],
                 log: Callable[[str], None], warn: Callable[[str], None]):
        if mode not in ("replay", "record", "live"):
            raise ValueError(f"net mode must be replay, record or live, not {mode!r}")
        self.mode = mode
        self.dir = Path(fixtures_dir) / "net"
        self.secrets = {k: v for k, v in secrets.items() if isinstance(v, str) and v}
        self.log, self.warn = log, warn
        self.blocked: str | None = None

    # ---- secrets ----------------------------------------------------------------------------

    def _variants(self) -> list[tuple[re.Pattern, re.Pattern, str]]:
        """(str pattern, bytes pattern, placeholder), longest first so a secret containing another wins.
        Percent escapes match in either case: servers and apps write %2f as well as %2F."""
        pairs: dict[str, str] = {}
        for name, value in self.secrets.items():
            for v in (value, quote(value, safe=""), quote(value, safe="@/:"), quote_plus(value, safe="")):
                pairs.setdefault(v, "{{%s}}" % name)
        out = []
        for v, placeholder in sorted(pairs.items(), key=lambda kv: -len(kv[0])):
            parts = re.split(r"(%[0-9A-F]{2})", v)
            pat = "".join("%" + "".join(f"[{c}{c.lower()}]" if c.isalpha() else c for c in part[1:])
                          if i % 2 else re.escape(part) for i, part in enumerate(parts))
            out.append((re.compile(pat), re.compile(pat.encode()), placeholder))
        return out

    def redact(self, text: str) -> str:
        for pat, _, placeholder in self._variants():
            text = pat.sub(lambda _m: placeholder, text)
        return _USERINFO_PASSWORD.sub(r"\1:{{userinfo}}@", text)

    def _redact_bytes(self, data: bytes) -> bytes:
        for _, pat, placeholder in self._variants():
            data = pat.sub(lambda _m: placeholder.encode(), data)
        return _USERINFO_PASSWORD_B.sub(rb"\1:{{userinfo}}@", data)

    def expand(self, text: str) -> str:
        """`{{name}}` placeholders (from seeded files) -> real secret values for a live request."""
        return _PLACEHOLDER.sub(lambda m: self.secrets.get(m[1], m[0]), text)

    # ---- keys and fixture files -------------------------------------------------------------

    def fixture_key(self, method: str, url: str, body: bytes | None) -> str:
        method = method.upper()
        key = f"{method} {self.redact(url)}"
        if method == "POST" and body:
            key += " " + _sha(self._redact_bytes(body))
        return key

    def _stem(self, method: str, url: str, body: bytes | None) -> str:
        host = re.sub(r"[^A-Za-z0-9.-]", "_", urlsplit(self.redact(url)).netloc.rpartition("@")[2]) or "nohost"
        return f"{method.upper()}_{host}_{_sha(self.fixture_key(method, url, body).encode())[:8]}"

    def _load(self, path: Path) -> dict | None:
        try:
            d = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, ValueError) as e:
            problem = f"cannot be read ({type(e).__name__})"
        else:
            problem = _fixture_problem(d)
        if problem is None:
            return d
        self.warn(f"net: fixture {path.name} is not valid ({problem}), ignored")
        return None

    def _find(self, method: str, url: str, body: bytes | None) -> Response | None:
        if not self.dir.is_dir():
            return None
        shown = self.redact(url)
        sha = _sha(self._redact_bytes(body)) if method == "POST" and body else None
        found = []
        for path in sorted(self.dir.glob("*.json")):
            d = self._load(path)
            if not d:
                continue
            req, how = d["request"], d.get("match", "exact")
            if str(req["method"]).upper() != method:
                continue
            u = req["url"]
            if how == "prefix":
                ok = shown.startswith(u)
            elif how == "glob":
                ok = fnmatch.fnmatchcase(shown, u)
            else:
                ok = shown == u and req.get("body_sha256") == sha
            if ok:
                found.append((how != "exact", path, d))
        if not found:
            return None
        _, path, d = min(found, key=lambda f: f[0])
        r = d["response"]
        body_file = r.get("body_file")
        if body_file:
            f = path.parent / Path(body_file).name
            try:
                data = f.read_bytes()
            except OSError:
                self.warn(f"net: fixture {path.name}: body_file {f.name} not found, the body is empty")
                data = b""
        else:
            data = (r.get("body") or "").encode("utf-8")
        headers = {str(k).lower(): str(v) for k, v in (r.get("headers") or {}).items()}
        return Response(r.get("status", 200), headers, data, None, round(r.get("delay_ms", DEFAULT_DELAY_MS)))

    def _save(self, method: str, url: str, headers: dict, body: bytes | None, resp: Response) -> None:
        stem = self._stem(method, url, body)
        sha = _sha(self._redact_bytes(body)) if method == "POST" and body else None
        safe_req = {k: self.redact(v) for k, v in headers.items() if not SENSITIVE_HEADER.search(k)}
        safe_resp = {k.lower(): self.redact(v) for k, v in resp.headers.items() if not SENSITIVE_HEADER.search(k)}
        self.dir.mkdir(parents=True, exist_ok=True)
        (self.dir / f"{stem}.body").write_bytes(self._redact_bytes(resp.body or b""))
        doc = {"request": {"method": method, "url": self.redact(url), "body_sha256": sha, "headers": safe_req},
               "match": "exact",
               "response": {"status": resp.status, "headers": safe_resp, "body_file": f"{stem}.body",
                            "delay_ms": resp.delay_ms}}
        (self.dir / f"{stem}.json").write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        self.log(f"net: recorded {self.dir.name}/{stem}.json")

    # ---- requests ---------------------------------------------------------------------------

    def request(self, method: str, url: str, headers: dict, body: bytes | None, timeout_ms: int) -> Response | None:
        method = method.upper()
        shown = self.redact(url)
        if method not in ("GET", "POST"):
            self.warn(f"net: {method} {shown} never gets an answer on the reader (only GET and POST work); "
                      "send it as POST with an override header")
            return None
        if self.blocked:
            return Response(0, {}, None, "secrets_invalid")
        if self.mode == "replay":
            resp = self._find(method, url, body)
            if resp is None:
                self.warn(f"net: no fixture for {method} {shown}; run with --net record to save one, "
                          "or write it by hand in " + str(self.dir))
                return Response(0, {}, None, "no_fixture", DEFAULT_DELAY_MS)
            self.log(f"net: {method} {shown} -> {resp.status} (fixture)")
            return resp
        if self.mode == "record":
            self._warn_unknown_secrets(url)
        resp = self._fetch(method, url, headers, body, timeout_ms)
        self.log(f"net: {method} {shown} -> {resp.status or resp.err} ({self.mode})")
        if self.mode == "record" and resp.err is None:
            self._save(method, url, headers, body, resp)
        return resp

    def _fetch(self, method: str, url: str, headers: dict, body: bytes | None, timeout_ms: int) -> Response:
        real_url = self.expand(url)
        real_headers = {k: self.expand(v) for k, v in headers.items()}
        data = self.expand_bytes(body) if body is not None else None
        req = urllib.request.Request(real_url, data=data, method=method, headers=real_headers)
        timeout = (timeout_ms if timeout_ms and timeout_ms > 0 else DEFAULT_TIMEOUT_MS) / 1000
        try:
            try:
                with urllib.request.urlopen(req, timeout=timeout) as r:
                    return self._response(r.status, r.headers, r.read())
            except urllib.error.HTTPError as e:
                with e:
                    return self._response(e.code, e.headers, e.read())
        except (TimeoutError, socket.timeout):
            return Response(0, {}, None, "timeout")
        except (urllib.error.URLError, OSError, ValueError, http.client.HTTPException) as e:
            reason = getattr(e, "reason", e)
            return Response(0, {}, None, "timeout" if isinstance(reason, socket.timeout) else "no_network")

    def expand_bytes(self, data: bytes) -> bytes:
        return _PLACEHOLDER.sub(lambda m: self.secrets[m[1]].encode() if m[1] in self.secrets else m[0].encode(),
                                data.decode("latin-1")).encode("latin-1") if b"{{" in data else data

    @staticmethod
    def _response(status: int, msg, body: bytes) -> Response:
        headers: dict[str, str] = {}
        for k, v in msg.items():
            k = k.lower()
            headers[k] = f"{headers[k]}, {v}" if k in headers else v
        return Response(status, headers, body, None, DEFAULT_DELAY_MS)

    def _warn_unknown_secrets(self, url: str) -> None:
        parts = urlsplit(self.redact(url))
        candidates = [("path segment", s) for s in parts.path.split("/")]
        candidates += [("query value", v.partition("=")[2]) for v in parts.query.split("&")]
        for what, text in candidates:
            if _SECRET_LIKE.fullmatch(text):
                self.warn(f"net: a {what} of {parts.scheme}://{parts.netloc} looks like a secret "
                          "but is not in secrets.json; it will be saved in the fixture as is")


def _number(v) -> bool:
    return isinstance(v, (int, float)) and not isinstance(v, bool)


def _fixture_problem(d) -> str | None:
    """Why a hand-written fixture cannot be used, or None."""
    if not isinstance(d, dict) or not isinstance(d.get("request"), dict) or not isinstance(d.get("response"), dict):
        return "needs \"request\" and \"response\" objects"
    req, resp = d["request"], d["response"]
    if not isinstance(req.get("method"), str) or not isinstance(req.get("url"), str):
        return "request.method and request.url must be strings"
    if req.get("body_sha256") is not None and not isinstance(req["body_sha256"], str):
        return "request.body_sha256 must be a string"
    if d.get("match", "exact") not in ("exact", "prefix", "glob"):
        return "match must be exact, prefix or glob"
    if not (isinstance(resp.get("status", 200), int) and _number(resp.get("status", 200))):
        return "response.status must be an integer"
    if not (_number(resp.get("delay_ms", 0)) and 0 <= resp.get("delay_ms", 0) < float("inf")):
        return "response.delay_ms must be a number >= 0"
    if resp.get("headers") is not None and not isinstance(resp["headers"], dict):
        return "response.headers must be an object"
    for k in ("body", "body_file"):
        if resp.get(k) is not None and not isinstance(resp[k], str):
            return f"response.{k} must be a string"
    return None


def _load_secrets(emu: "Emu") -> tuple[dict[str, str], list[str]]:
    """(usable secrets, problems); a problem never contains a secret value."""
    f = emu.secrets_file.name
    try:
        data = json.loads(emu.secrets_file.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return {}, []
    except (OSError, ValueError) as e:
        return {}, [f"{f} cannot be read ({type(e).__name__}); no secrets are applied"]
    if not isinstance(data, dict):
        return {}, [f"{f} must be a JSON object; no secrets are applied"]
    secrets, problems = {}, []
    for k, v in data.items():
        if isinstance(v, (int, float)) and not isinstance(v, bool):
            v = str(v)
        if not re.fullmatch(_NAME, k):
            problems.append(f"{f}: key {k!r} is not a valid name (letters, digits, _ . -); its value is not redacted")
        elif not isinstance(v, str) or not v:
            problems.append(f"{f}: value of {k!r} must be a non-empty string or a number; it is not redacted")
        else:
            secrets[k] = v
    return secrets, problems


def install(emu: "Emu") -> None:
    if not emu._net_enabled():
        return
    L, ctx = emu.L, emu._ctx
    secrets, problems = _load_secrets(emu)
    for p in problems:
        emu._warn("net: " + p)
    net = Net(emu.net_mode, emu.fixtures_dir, secrets, emu._log_line, emu._warn)
    if problems and emu.net_mode == "record":
        net.blocked = "recording refused: fix secrets.json first, otherwise a secret could be saved in a fixture"
        emu.errors.append("net: " + net.blocked)
        emu._log_line("ERROR net: " + net.blocked)
    opts_of = L.execute(_OPTS)

    def lua_response(r: Response):
        t = {"status": r.status, "code": r.status, "headers": dict(r.headers)}
        if r.err is not None:
            t["err"] = t["error"] = r.err
        if r.body is not None:
            t["body"], t["bytes"] = r.body, len(r.body)
        return L.table_from(t, recursive=True)

    def request(opts=None, fn=None, *_):
        check_arg("request", 1, opts, "table")
        check_arg("request", 2, fn, "function")
        o = opts_of(opts)
        url = raw(o["url"])
        if url is None:
            raise TypeError("bad extra argument (string expected)")
        method = raw(o["method"]) if o["method"] is not None else b"GET"
        timeout = o["timeout_ms"]
        try:
            url_s, method_s = url.decode("utf-8"), (method or b"").decode("ascii")
        except UnicodeDecodeError:
            return None, "invalid_argument"
        if not _URL.match(url_s) or not method_s:
            return None, "invalid_argument"
        hdrs = {}
        for k, v in o["headers"].items():
            try:
                hdrs[raw(k).decode("utf-8")] = raw(v).decode("utf-8")
            except UnicodeDecodeError:
                emu._warn("net: a header that is not valid UTF-8 was dropped")
        body = raw(o["body"]) if o["body"] is not None else None
        resp = net.request(method_s, url_s, hdrs, body, round(timeout) if timeout else 0)
        if resp is None:
            return make_object(L, "job", {"cancel": lambda *a: None})
        active = [True]

        def done():
            if active[0]:
                active[0] = False
                emu.net_inflight -= 1

        def deliver():
            try:
                emu._protected(fn, ctx, lua_response(resp), where="ctx.net:request callback")
            finally:
                done()

        timer = emu._add_timer(resp.delay_ms, deliver)
        emu.net_inflight += 1

        def cancel(*_):
            emu._cancel_timer(timer)
            done()

        return make_object(L, "job", {"cancel": cancel})

    def keep_wifi(ms=None, *_):
        check_arg("keep_wifi", 1, ms, "number")
        emu._log_line(f"net: keep_wifi {ms:g} ms")

    def websocket(*_):
        raise RuntimeError("ctx.net:websocket is not emulated")

    ctx.net = make_object(L, "net", {"request": request, "keep_wifi": keep_wifi, "websocket": websocket})
