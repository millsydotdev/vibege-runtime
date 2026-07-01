# Changelog

All notable changes to the VibeGE platform will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.2.0-alpha.1] — 2026-07-01

### Added
- Initial alpha release of the VibeGE platform
- Runtime engine with 14 crates (window, input, renderer, audio, IPC, sandbox, suspension, tray, config, SDK, scene, asset, app)
- Lua SDK with 88 API functions across 12 modules
- Scene system with boot, home, library, store, settings, game scenes
- Suspension engine with SHA256 integrity and Zstd compression
- CLI tool with 9 commands (new, dev, build, publish, install, validate, doctor, ai)
- Backend API with 21 routes (auth, registry, moderation)
- Website with 11 pages (home, games, dashboard, admin, docs, download, auth)
- CI/CD with 7 GitHub Actions workflows
- 4 sample Lua games (Pong, Solitaire, Spider, Overlay Test)
