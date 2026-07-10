---
title: Cadspec
description: CAD as code — declarative CAD in TOML, live browser preview, deterministic DXF output.
order: 1
---

# Cadspec

Cadspec is a **CAD as code** CLI tool and Rust library for
declarative CAD modeling. Write geometry as code in `.cf` TOML files,
watch it live in the browser, and compile to AutoCAD-compatible DXF —
built for humans and AI agents working together.

## The plan is not drawn — it is declared

Traditional CAD drawings are collections of lines without semantics:
impossible to version, diff or automate. Cadspec treats an architectural
plan like source code:

- Same input → **bit-identical output**, every time.
- `git diff` works on floor plans.
- AI agents can generate and modify designs with surgical precision.

## The core loop

```bash
cadspec new casa && cd casa
cadspec serve --open        # live preview in the browser
```

Edit any `.cf` file — by hand or by asking an AI agent — and the browser
updates on every save. Parse errors and constraint violations appear as an
overlay instead of a crash. When the design is right, `cadspec build`
emits a deterministic DXF.

## Built for agents

Agents get first-class support:

- `cadspec schema` — the full `.cf` language reference in one command;
  agents self-discover the format without prior training.
- `cadspec check --json` / `cadspec layers --json` — machine-readable
  reports.
- `cadspec preview` — a faithful PNG render plus `preview.meta.json`
  with per-entity bounding boxes, so multimodal agents can *look* at the
  plan and locate every entity.
- `cadspec preview --highlight ln-001,tx-002` — labeled markers to
  visually confirm an edit landed where intended.

## How the documentation is organized

- [Installation](installation.md) — install, update and uninstall.
- [Quick Start](quickstart.md) — from zero to DXF.
- [The .cf Format](cf-format.md) — layers, primitives, construction tools.
- [Live Preview](live-preview.md) — `cadspec serve` and its controls.
- [Building & Export](building-and-export.md) — build, watch, DXF import/export.
- [Working with Agents](agents.md) — the agent-facing toolchain.
- [CLI Reference](cli-reference.md) — every command and flag.

## Part of UniverLab

Cadspec is an experiment of [UniverLab](https://github.com/UniverLab),
an open computational laboratory. It follows the lab's engineering
principles: one tool one job, reproducibility first, offline-friendly
design.
