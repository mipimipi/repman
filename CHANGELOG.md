# Changelog

## [Release 0.6.0](https://gitlab.com/mipimipi/repman/tags/v0.6.0) (2023-05-01)

### Added

- Docker images for x86_64 and AArch64

### Removed

- Support for Armv7h since I do not have the possibility to test repman for this architecture

## [Release 0.5.0](https://gitlab.com/mipimipi/repman/tags/v0.5.0) (2023-04-10)

### Added

- Support for Google Cloud Storage: repman can be used to manage custom repositories that are hosted there
- Option `--force-no-version` for `repman update` to force an update of packages that are not tied to a specific version but that are built from a VCS such as git. So far, for updating such packages they had to be re-added with `repman add`.

## [Release 0.4.0](https://gitlab.com/mipimipi/repman/tags/v0.4.0) (2023-04-09)

### Added

- Support for AWS S3: repman can be used to manage custom repositories that are hosted on AWS S3

## [Release 0.3.0](https://gitlab.com/mipimipi/repman/tags/v0.3.0) (2023-01-29)

### Added

- Flag -A / --ignorearch. The new flag allows ignoring the architecture maintained in PKGBUILD (valid for `repman add` and `repman update`). Some AUR packages are restricted to x86_64 per the `arch` array in their PKGBUILD  file though they build and run perfectly on aarch64, for example. In this case, the new flag can be used to add such packages to a aarch64 repository without changing the PKGBUILD.

## [Release 0.2.0](https://gitlab.com/mipimipi/repman/tags/v0.2.0) (2023-01-08)

### Added

- $db as placeholder for the DB name in the configuration file.

## [Release 0.1.0](https://gitlab.com/mipimipi/repman/tags/v0.1.0) (2022-11-26)

Initial release supporting almost all features of crema v3.1.1, except the `--ignorearch` flag. 
