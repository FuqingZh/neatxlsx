# neatxlsx Repository Agent Map

## Authority

- Read `docs/README.md` first for current architecture, API, migration, and
  testing authority.
- Treat the public API contract and tests as one compatibility boundary.
- `crates/neatxlsx_core` owns XLSX mechanics; `crates/neatxlsx_py` contains
  bridge code only; `python/neatxlsx` owns caller-facing policy and lifecycle.

## Environment

- Use Python 3.13 locally and support Python 3.11+.
- Install development dependencies with `pdm sync -G dev`; PDM uses its uv
  backend and `pdm.lock` is the only committed Python lock file.
- Build the local extension with `pdm run maturin develop`.

## Validation

- Run the smallest affected test first.
- Run the complete local gate with `pdm run check`.
- Before delivery, also run `git diff --check` and `git status --short`.
- Use `pdm run select-checks --base <sha> --head <sha>` to explain affected
  lanes for a commit range.

## Delivery And Safety

- Keep axiomkit independent; do not add a neatxlsx dependency or remove its
  retained XLSX implementation.
- Do not publish packages, create tags, push images, or create the remote
  repository without explicit authorization.
- Preserve transactional replacement, literal string writes, ZIP64 defaults,
  and the Python/PyO3 bridge contract when changing public behavior.

