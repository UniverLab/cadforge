---
title: Installation
description: Install cadforge with the quick installer, cargo, or from source.
order: 2
---

# Installation

> Cadforge is currently in **beta** (`0.1.0-beta.x`). Interfaces may still
> change before 1.0.

## Quick install

**Linux / macOS:**

```bash
curl -fsSL https://raw.githubusercontent.com/UniverLab/cadforge/main/scripts/install.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/UniverLab/cadforge/main/scripts/install.ps1 | iex
```

## Via cargo

```bash
cargo install cadforge
```

Available on [crates.io](https://crates.io/crates/cadforge).

## From source

```bash
git clone https://github.com/UniverLab/cadforge.git
cd cadforge
cargo build --release
# Binary at target/release/cadforge
```

## Uninstall

```bash
rm -f ~/.local/bin/cadforge
```

Projects are plain directories of TOML files — nothing else to clean up.
