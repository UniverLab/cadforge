---
title: Working with Agents
description: The agent-facing toolchain — schema discovery, JSON reports, visual previews with metadata.
order: 7
---

# Working with Agents

Cadforge is designed for **humans and AI agents working together** on the
same project. Everything an agent needs is exposed through the CLI.

## Self-discovery — `cadforge schema`

```bash
cadforge schema
```

Prints the complete `.cf` language reference as markdown. An agent with
shell access can learn the entire format in one command — no prior
training on cadforge required.

## Machine-readable state

```bash
cadforge check --json    # validation report
cadforge layers --json   # layers, entity counts, colors
```

## Visual grounding — `cadforge preview`

```bash
cadforge preview                          # PNG + preview.meta.json
cadforge preview --format svg             # same render as vector SVG
cadforge preview --width 1024 -H 768      # custom resolution
cadforge preview --layer muros            # single layer
```

The PNG is a faithful render — real text, measured dimension labels,
hatches, line styles — and `preview.meta.json` contains per-entity
bounding boxes in **world and pixel coordinates**. A multimodal agent can
look at the image and locate every entity in it.

Rendering is deterministic on any machine (an embedded monospace font is
used), including fontless containers.

## Confirming edits — `--highlight`

```bash
cadforge preview --highlight ln-001,tx-002
```

Draws labeled amber markers around the listed entities, so an agent can
visually confirm its edit landed exactly where intended.

## Targeted edit prompts from the browser

In [live preview](live-preview.md), clicking an entity and pressing
**copy for agent** produces a ready-made prompt with the entity's id,
source TOML and file — paste it into your agent and ask for the change.

## A typical agent loop

```bash
cadforge schema                      # 1. learn the language
cadforge layers --json               # 2. inspect the project
# ... edit .cf files ...
cadforge check --json                # 3. validate
cadforge preview --highlight <ids>   # 4. visually confirm
cadforge build                       # 5. emit the DXF
```
