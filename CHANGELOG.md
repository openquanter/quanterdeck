# Changelog

## Unreleased

### M0 — skeleton

- Runtime adapter protocol, standard library only, discovered by entry point.
- `legacy_py` adapter for OpenQuanter 1.x: services via `manager.sh`,
  configuration with backups and conflict detection, strategy parameter
  discovery through an out-of-process probe.
- `oq_cli` adapter placeholder for 2.0, reporting honest capabilities
  while the tools have no machine-readable output.
- FastAPI surface at `/api/v1`, capability-driven web shell, seventeen
  routes stubbed to their milestones.
- Read-only by default; loopback-only by default; both need a deliberate
  act to relax.
