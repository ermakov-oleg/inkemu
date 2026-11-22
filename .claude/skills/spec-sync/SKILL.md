---
name: spec-sync
description: Use when the official InkApp API contract has a new tag or changed sections, when a "spec-sync" GitHub issue is open, or when asked to check the emulator against the spec. Re-syncs inkemu's conformance checks, deviations and contract lock.
---

# Re-sync inkemu with the official API contract

All paths are relative to the repository root. The lock `spec/contract.lock` holds only section ids, hashes and the
tag. Never commit contract text, not even in tests, fixtures, comments or issue bodies: write rules in your own words
and cite sections as `§N`.

1. See what changed: `uv run python tools/spec_sync.py` (add `--diff` to print the text diff of changed sections; read
   it, do not paste it anywhere). `--check` exits 1 on drift, 2 on a network error.
2. Map each change to rules. Find the checks for the section under `tests/conformance/<SS>/` (`<SS>` is the section
   number) and read `tests/conformance/README.md` for the check format. Write new or adjusted checks first; they go
   red when the core does not follow the change yet.
3. List every new red check in `tests/conformance/pending.json` (id and a one-line reason) and describe the gap in
   `tests/conformance/GAPS.md`. Then fix the core, or hand the list to whoever owns it, until
   `uv run python tools/conformance.py --strict` and `uv run pytest -q` are green. Remove pending entries as checks
   pass.
4. If the change is about device behaviour that may differ from the contract, measure it before deciding. Use a reader,
   or a QEMU emulator of the real firmware if the maintainer has one. Write a small probe app that records its results
   (storage or log), run it, read them back. Without a measurement inkemu follows the contract. Do not commit dumps or
   local paths; describe the probe as "a probe app run on firmware <build> (<date>)".
5. Update `spec/deviations.md`: a measured difference gets the next `D<n>` (contract says, reader does, inkemu does,
   status, check ids) and the check carries `"deviation": "D<n>"` with the contract's `spec_value`. Unmeasured ones are
   notes marked "unverified".
6. Record the new contract last: `uv run python tools/spec_sync.py update`, then commit `spec/contract.lock`.

Before every commit run `uv run pytest -q tests/test_conformance.py tests/test_spec_sync.py` and
`uv run ruff check && uv run ruff format --check`.

Commits are one line, in English, with explicit `git add <paths>` (never `git add -A`). Example:
`Follow API 2.34: new checks for section 6, lock updated`.
