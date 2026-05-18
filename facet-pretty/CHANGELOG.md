# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Replaced the Tokyo Night palette with a faithful [Melange](https://github.com/savq/melange-nvim) colour scheme. The terminal background is detected once (`detect-terminal-theme` feature, on by default) to pick the light or dark variant; override with `Theme`/`Palette` or the `FACET_PRETTY_THEME` env var (`dark`/`light`). Per-shape scalar colours now draw from the palette accents instead of random HSL hues. The `ColorGenerator`/`RGB` API and `tokyo_night` module are removed in favour of `Palette`/`Theme`.

## [0.44.7](https://github.com/facet-rs/facet/compare/facet-pretty-v0.44.6...facet-pretty-v0.44.7) - 2026-04-14

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.44.6](https://github.com/facet-rs/facet/compare/facet-pretty-v0.44.5...facet-pretty-v0.44.6) - 2026-04-13

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.44.5](https://github.com/facet-rs/facet/compare/facet-pretty-v0.44.4...facet-pretty-v0.44.5) - 2026-04-13

### Other

- updated the following local packages: facet-reflect

## [0.44.4](https://github.com/facet-rs/facet/compare/facet-pretty-v0.44.3...facet-pretty-v0.44.4) - 2026-03-29

### Other

- Add collection truncation to facet-pretty
