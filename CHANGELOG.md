# Changelog

## [Release 0.3.0](https://gitlab.com/mipimipi/repman/tags/v0.3.0) (2023-01-29)

### Added

- Flag -A / --ignorearch. The new flag allows ignoring the architecture maintained in PKGBUILD (valid for `repman add` and `repman update`). Some AUR packages are restricted to x86_64 per the `arch` array in their PKGBUILD  file though they build and run perfectly on aarch64, for example. In this case, the new flag can be used to add such packages to a aarch64 repository without changing the PKGBUILD.

## [Release 0.2.0](https://gitlab.com/mipimipi/repman/tags/v0.2.0) (2023-01-08)

### Added

- $db as placeholder for the DB name in the configuration file.

## [Release 0.1.0](https://gitlab.com/mipimipi/repman/tags/v0.1.0) (2022-11-26)

Initial release supporting almost all features of crema v3.1.1, except the `--ignorearch` flag. 
