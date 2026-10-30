# Changelog

## [Unreleased]

## [0.1.0] - 2026-10-30

### Added

- add the CI action
- read anonymously when the job has no ID token
- let the action read private caches and push only to writable ones
- download the matching client in the action
- verify the client against the pinned digest
- upload from the action while the job runs

### Changed

- Update calf-granary-client to 0.2.0

### Fixed

- keep the hashserv token out of the BitBake configuration
- upload only finished files and keep going after failures

### Other

- rename the action metadata to action.yaml
- read only the ID token variables in the action
- format the action with Prettier
- release the action as its own unit
- pin the client digests in the action
