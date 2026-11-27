# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed
- `git-core` / `cep` / `sarment`: Avoid scanning all `refs/remotes/origin` refs when collecting local branch sync status and batch `cep list` branch and agent-marker lookups for faster execution in large repositories.

## [0.1.1] - 2026-11-27

## [0.1.0] - 2026-11-26

### Added
- `vignoble`: Suite management CLI (`update`, `postinstall`, `shell-init`, `plugin`, and multicall dispatch for `sarment` and `cep`).
- `sarment`: Branch stage status and cleanup CLI (`all`, `list`, `clean`) with cascading `.vignoble` configuration.
- `cep`: Long-living, reusable git worktree pool manager (`list`, `create`, `cd`, `detach`, `claim`, `release`, `remove`).

[Unreleased]: https://github.com/themouette/vignoble/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/themouette/vignoble/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/themouette/vignoble/releases/tag/v0.1.0
