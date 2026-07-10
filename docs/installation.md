---
title: Installation
description: Install cadspec with the quick installer, cargo, or from source.
order: 2
---

# Installation

> Cadspec is currently in **beta** (`0.1.0-beta.x`). Interfaces may still
> change before 1.0.

## Quick install

**Linux / macOS:**

```bash
curl -fsSL https://raw.githubusercontent.com/UniverLab/cadspec/main/scripts/install.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/UniverLab/cadspec/main/scripts/install.ps1 | iex
```

## Via cargo

```bash
cargo install cadspec
```

Available on [crates.io](https://crates.io/crates/cadspec).

## From source

```bash
git clone https://github.com/UniverLab/cadspec.git
cd cadspec
cargo build --release
# Binary at target/release/cadspec
```

## Uninstall

```bash
rm -f ~/.local/bin/cadspec
```

Projects are plain directories of TOML files — nothing else to clean up.
