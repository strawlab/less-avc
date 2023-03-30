# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.1.3]: https://github.com/strawlab/less-avc/releases/tag/0.1.3
[0.1.2]: https://github.com/strawlab/less-avc/releases/tag/0.1.2
[0.1.1]: https://github.com/strawlab/less-avc/releases/tag/0.1.1
[0.1.0]: https://github.com/strawlab/less-avc/releases/tag/0.1.0
