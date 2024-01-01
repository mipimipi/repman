[![REUSE status](https://api.reuse.software/badge/gitlab.com/mipimipi/repman)](https://api.reuse.software/info/gitlab.com/mipimipi/repman)
[![Crates.io](https://img.shields.io/crates/v/repman.svg)](https://crates.io/crates/repman)

# Repman

[Custom repositories](https://wiki.archlinux.org/index.php/Pacman/Tips_and_tricks#Custom_local_repository) are personal [Arch Linux](https://www.archlinux.org/) repositories that can contain packages from the Arch Linux user repository ([AUR](https://aur.archlinux.org/)) or other packages (local packages, for example, where the [PKGBUILD file](https://wiki.archlinux.org/index.php/PKGBUILD) is stored in the local file system). repman (**REP**ository **MAN**ager) helps to manage them, whether they are local or remote.

repman replaces [crema](https://gitlab.com/mipimipi/crema) which is no longer under development and adds new features. It can run on
x86_64/AMD64 and AArch64/ARM64 and thus can manage repositories for these architectures.

Some use cases for custom repositories:

- During the installation of Arch Linux you want to pacstrap AUR packages. Therefore, these packages must be provided by a repository.
- You want to use self-defined [meta packages](https://nerdstuff.org/posts/2020/2020-002_meta_packages/) to make an Arch Linux installation more efficient
- You want to deploy your packages to a repository automatically as part of a CI/CD pipeline
- You are running Arch Linux on multiple machines in your local network and want to provide all of them with packages / package updates from a custom repository in your local network

# Features

repman supports different storage locations for repositories (check [Optional dependencies](#optional-dependencies)):

- Local file system
- Remote servers which are accessible via SSH
- [AWS S3](https://docs.aws.amazon.com/AmazonS3/latest/userguide/Welcome.html)
- [Google Cloud Storage](https://cloud.google.com/storage)

It can be used for the following tasks:

- Adding (this includes also building) packages
- Removing packages
- Updating packages

A very important goal of repman is to keep the local system - i.e., the system that is used to manage custom repositories - as clean as possible. Therefore, packages are built in
[chroot](https://wiki.archlinux.org/index.php/Chroot) containers via [makechrootpkg](https://wiki.archlinux.org/index.php/DeveloperWiki:Building_in_a_clean_chroot) per default.

# Installation

## From [AUR](https://aur.archlinux.org/)

There are AUR packages for repman: [repman](https://aur.archlinux.org/packages/repman/) and [repman-git](https://aur.archlinux.org/packages/repman-git/). They can be installed with [AUR helpers](https://wiki.archlinux.org/title/AUR_helpers) such as [trizen](https://github.com/trizen/trizen).

## Manually from Sources

Another option is a manual installation. For this execute:

    $ git clone https://gitlab.com/mipimipi/repman.git
    $ cd repman
    $ make
    $ sudo make install

## Docker

There are [Docker images](https://hub.docker.com/repository/docker/mipimipi/repman) for x86_64 and AArch64 architectures that contain repman. These images can be used in CI pipelines, for example. To download the latest image execute:

    $ docker pull mipimipi/repman:latest

## Optional dependencies

Depending on what repman is used for and how, some additional dependencies are required:

- To handle packages from AUR, [git](https://wiki.archlinux.org/title/Git) is required
- To sign packages or repository databases, [GnuPG](https://wiki.archlinux.org/title/GnuPG) is required
- To manage remote repositories, depending on the type of the server/the access to the server, specific tools are required:
    - Access via SSH requires [rsync](https://wiki.archlinux.org/title/Rsync) and [OpenSSH](https://wiki.archlinux.org/title/OpenSSH)
    - AWS S3 requires s3cmd (for [x86_64](https://archlinux.org/packages/extra/any/s3cmd/), for [AArch64](https://archlinuxarm.org/packages/any/s3cmd))
    - Google Cloud Storage requires [google-cloud-cli](https://aur.archlinux.org/packages/google-cloud-cli)
- In case distributed builds are used, [distcc](https://wiki.archlinux.org/title/Distcc) is required	

# Configuration

repman requires information about the repositories, such as name and (remote) path. This is stored in the configuration file `$XDG_CONFIG_HOME/repman/repos.conf` in [TOML format](https://en.wikipedia.org/wiki/TOML). See repman’s [man page](doc/manpage.adoc) for details.

# Usage

Execute `repman help` to get information about how to call repman. repman’s [man page](doc/manpage.adoc) contains comprehensive documentation: `$ man repman`

# Troubeshooting

See the [troubleshooting chapter](doc/manpage.adoc#user-content-troubleshooting-and-faq) in repman’s [man page](doc/manpage.adoc).

# Details

repman utilizes tools like [makechrootpkg](https://wiki.archlinux.org/index.php/DeveloperWiki:Building_in_a_clean_chroot), [makepkg](https://www.archlinux.org/pacman/makepkg.8.html), [repo-add](https://www.archlinux.org/pacman/repo-add.8.html), and repo-remove. [rsync](https://wiki.archlinux.org/index.php/Rsync), or vendor-specific tools such as [s3cmd](https://github.com/s3tools/s3cmd) or [gsutil](https://cloud.google.com/storage/docs/gsutil) are used to
transfer repositories between remote locations and the local file system. The local copies are manipulated with the above-mentioned tools.

# License

[GNU Public License v3.0](https://gitlab.com/mipimipi/repman/blob/master/LICENSE)
