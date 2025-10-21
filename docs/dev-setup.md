# Dev Setup

## Toolchains
- **Rust:** stable (via `rust-toolchain.toml`), components: rustfmt, clippy
- **Node:** 20.18.0 (via `.nvmrc`)
- **npm:** auto from Node 20.x

## Quickstart
```bash
# Rust
rustup show
cargo --version
cargo fmt --version
cargo clippy --version

# Node
nvm use
node -v
npm -v
```

## Build sanity check

```bash
cargo build -p ingestor -p api
npm run npm:version
```
