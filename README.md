# 囲碁 — Go Game

A browser-based Go (Baduk) game built in Rust compiled to WebAssembly.

## Playing

Visit the live game at: **https://alphonsev7.github.io/gogame**

## Rules (Japanese)

- Black plays first, players alternate placing stones
- Stones with no liberties are captured and removed
- Ko rule: you may not recreate the previous board position
- Suicide (placing with no liberties) is forbidden
- Two consecutive passes end the game

## Project Structure

```
src/
  lib.rs      — WASM entry point
  board.rs    — Board state, groups, capture logic + tests
  game.rs     — Game rules, turn management + tests
www/
  index.html  — Page shell
  style.css   — Zen/wooden visual style
  main.js     — Canvas rendering, user input
.github/workflows/
  ci.yml      — Tests → Build → Deploy pipeline
```

## Development

All development happens via Claude Code on the web. Every push to `main`:
1. Runs `cargo test` — if any test fails, deploy is blocked
2. Builds Rust → WebAssembly via `wasm-pack`
3. Deploys to GitHub Pages automatically
