# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.6 - unreleased

### Changed

- Require rust 1.73

## [0.1.5] - 2023-08-29

### Fixed

- Fixed compilation of backtraces on recent nightly.
- Tests parse ffmpeg version "6.0" as "6.0.0".

## [0.1.4] - 2023-05-25

### Fixed

- Fixed a bug in which specification was not followed. Reported by @wrv.
  Specifically, "nal_ref_idc shall not be equal to 0 for NAL units with
  nal_unit_type equal to 5".

### Added

- `no_std` support
- implement `source()` method for `Error`

## [0.1.3] - 2023-03-30

### Added

- Added `NalUnit::to_nal_unit()` method to save "naked" NAL units without start
  prefix framing.

## [0.1.2] - 2022-11-20

### Added

- Use memchr crate for SIMD-accelerated searching and thus improve performance.
- Tests requiring ffmpeg will fail with an explicit error if ffmpeg is too old.
  (Version 5.1 or higher is required for high bit-depth data.)

## [0.1.1] - 2022-11-19

### Added

- Added crate metadata

## [0.1.0] - 2022-11-19

### Added

- Initial release

[0.1.5]: https://github.com/strawlab/less-avc/releases/tag/0.1.5
[0.1.4]: https://github.com/strawlab/less-avc/releases/tag/0.1.4
[0.1.3]: https://github.com/strawlab/less-avc/releases/tag/0.1.3
[0.1.2]: https://github.com/strawlab/less-avc/releases/tag/0.1.2
[0.1.1]: https://github.com/strawlab/less-avc/releases/tag/0.1.1
[0.1.0]: https://github.com/strawlab/less-avc/releases/tag/0.1.0
