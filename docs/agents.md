---
title: Working with Agents
description: The agent-facing toolchain — schema discovery, JSON reports, visual previews with metadata.
order: 7
---

# Working with Agents

Cadspec is designed for **humans and AI agents working together** on the
same project. Everything an agent needs is exposed through the CLI.

## Self-discovery — `cadspec schema`

```bash
cadspec schema
```

Prints the complete `.cf` language reference as markdown. An agent with
shell access can learn the entire format in one command — no prior
training on cadspec required.

## Machine-readable state

```bash
cadspec check --json    # validation report
cadspec layers --json   # layers, entity counts, colors
```

## Visual grounding — `cadspec preview`

```bash
cadspec preview                          # PNG + preview.meta.json
cadspec preview --format svg             # same render as vector SVG
cadspec preview --width 1024 -H 768      # custom resolution
cadspec preview --layer muros            # single layer
```

The PNG is a faithful render — real text, measured dimension labels,
hatches, line styles — and `preview.meta.json` contains per-entity
bounding boxes in **world and pixel coordinates**. A multimodal agent can
look at the image and locate every entity in it.

Rendering is deterministic on any machine (an embedded monospace font is
used), including fontless containers.

## Confirming edits — `--highlight`

```bash
cadspec preview --highlight ln-001,tx-002
```

Draws labeled amber markers around the listed entities, so an agent can
visually confirm its edit landed exactly where intended.

## Targeted edit prompts from the browser

In [live preview](live-preview.md), clicking an entity and pressing
**copy for agent** produces a ready-made prompt with the entity's id,
source TOML and file — paste it into your agent and ask for the change.

## A typical agent loop

```bash
cadspec schema                      # 1. learn the language
cadspec layers --json               # 2. inspect the project
# ... edit .cf files ...
cadspec check --json                # 3. validate
cadspec preview --highlight <ids>   # 4. visually confirm
cadspec build                       # 5. emit the DXF
```
