// Performs the app's outgoing requests (take_outgoing) with fetch and returns the core's `deliver` arguments.
import { toBase64, fromBase64 } from './core.js';

const DEFAULT_TIMEOUT_MS = 15000;

export function corsLine(method, url) {
  return (
    `net: ${method} ${url} failed in the browser — the server does not allow requests from web pages (CORS) or is ` +
    'unreachable; record a fixture with `inkemu run --net record` and commit it under tests/fixtures/<app_id>/net/'
  );
}

/**
 * `request`: one entry of take_outgoing ({id, method, url, headers: [[k, v]], body_b64, timeout_ms}).
 * Returns `deliver` arguments; `log`, when present, is a line for the page log and is not part of them.
 */
export async function perform(request, fetchFn = globalThis.fetch) {
  const { id, method, url } = request;
  const ctl = new AbortController();
  // Not AbortSignal.timeout: its timer is unref'd in Node; this one also covers the body read.
  const timer = setTimeout(() => ctl.abort(), request.timeout_ms > 0 ? request.timeout_ms : DEFAULT_TIMEOUT_MS);
  try {
    const init = { method, headers: request.headers ?? [], signal: ctl.signal };
    // fetch throws a TypeError for a GET/HEAD body, which would read as a CORS failure
    if (request.body_b64 && method !== 'GET' && method !== 'HEAD') init.body = fromBase64(request.body_b64);
    const res = await fetchFn(url, init);
    const body = new Uint8Array(await res.arrayBuffer());
    return { id, status: res.status, headers: [...res.headers], body_b64: toBase64(body), delay_ms: 0 };
  } catch (e) {
    if (ctl.signal.aborted) return { id, error: 'timeout' };
    if (e instanceof TypeError) return { id, error: 'no_network', log: corsLine(method, url) };
    return { id, error: 'no_network', log: `net: ${method} ${url} failed: ${e?.message ?? e}` };
  } finally {
    clearTimeout(timer);
  }
}
