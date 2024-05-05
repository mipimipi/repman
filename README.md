[![REUSE status](https://api.reuse.software/badge/gitlab.com/mipimipi/repman)](https://api.reuse.software/info/gitlab.com/mipimipi/repman)
[![Crates.io](https://img.shields.io/crates/v/repman.svg)](https://crates.io/crates/repman)

# Repman

[Custom repositories](https://wiki.archlinux.org/index.php/Pacman/Tips_and_tricks#Custom_local_repository) are personal [Arch Linux](https://www.archlinux.org/) repositories that can contain packages from the Arch Linux user repository ([AUR](https://aur.archlinux.org/)) or other packages (local packages, for example, where the [PKGBUILD file](https://wiki.archlinux.org/index.php/PKGBUILD) is stored in the local file system). repman (**REP**ository **MAN**ager) helps to manage them, whether they are local or remote.

Some use cases for custom repositories:

- During the installation of Arch Linux you want to pacstrap AUR packages. Therefore, these packages must be provided by a repository.
- You want to use self-defined [meta packages](https://nerdstuff.org/posts/2020/2020-002_meta_packages/) to make an Arch Linux installation more efficient
- You want to deploy your packages to a repository automatically as part of a CI/CD pipeline
- You are running Arch Linux on multiple machines in your local network and want to provide all of them with packages / package updates from a custom repository in your local network

# Table of contents

- [Features](#features)
- [Installation](#installation)
    - [From AUR](#from-aur)
    - [Manually from sources](#manually-from-sources)
    - [Docker](#docker)
    - [Optional dependencies](#optional-dependencies)
- [Configuration](#configuration)
- [Usage](#usage)
    - [Hints](#hints)
    - [Troubleshooting](#troubeshooting)
- [Implementation details](#implementation-details)

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

There are AUR packages for repman: [repman](https://aur.archlinux.org/packages/repman/) and [repman-git](https://aur.archlinux.org/packages/repman-git/). They can be installed with [AUR helpers](https://wiki.archlinux.org/title/AUR_helpers) such as [trizen](https://github.com/trizen/trizen). These packages are available as binaries via the [nerdstuff repository](https://nerdstuff.org/repository/).

## Manually from sources

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

## Hints

### Speeding up the build process by storing chroot containers in main memory

The build process can be accelerated by using _tmpfs_ for _chroot_ containers. [tmpfs](https://en.wikipedia.org/wiki/Tmpfs) is a file system that resides in the main memory. It should only be used if sufficient memory is available since otherwise the [swap space](https://wiki.archlinux.org/title/Swap) will be used. tmpfs can be used for all chroot containers or only for chroot containers of dedicated repositories. To use it for all chroot containers, add the following line to `/etc/fstab`:

    tmpfs   /home/myuser/.cache/repman/chroots         tmpfs   rw,nodev,suid,size=4G          0  0

The mount path and the size must of course be adjusted to the specific context and needs.

### Distributed compiling (distcc)

Distributed builds in chroot containers can either be enabled before a container is created or after.

To enable it before a chroot container is created, execute the following steps:

1. Enable and configure distcc in the `makepkg.conf` file that is used for the chroot container (see the mkchroot command and the [distcc documentation in the Arch Linux Wiki](https://wiki.archlinux.org/title/Distcc) or the [Arch Linux ARM Wiki](https://archlinuxarm.org/wiki/Distributed_Compiling).

2. Install distcc on your system:
    
        $ pacman -Syu distcc

3. Create the chroot container:

        $ repman mkchroot --repo <REPOSITORY>
   
If a container already exists, execute the following steps:

1. Install distcc on your system and in the chroot container:

        $ pacman -Syu distcc    
        $ arch-nspawn ~/.cache/repman/chroots/<REPOSITORY>/root pacman -Syu distcc

2. Configure the chroot for distributed builds by adjusting `~/.cache/repman/chroots/<REPOSITORY>/root/etc/makepkg.conf` accordingly, see the distcc documentation in the [Arch Linux Wiki](https://wiki.archlinux.org/title/Distcc) or the [Arch Linux ARM Wiki](https://archlinuxarm.org/wiki/Distributed_Compiling).

3. Remove the old container copy and lock file: 

        $ cd ~/.cache/repman/chroots/<REPOSITORY>    
        $ sudo rm -rd <YOUR USER NAME> <YOUR USER NAME>.lock

### AWS S3

Some hints to configure the AWS S3 storage prior to use it with repman:

1. Create an AWS S3 account.
2. Create an S3 bucket with a folder structure of your desire to host the repository.
3. Make the bucket publicly readable.
4. Enable access control lists (ACL) for the bucket.
5. Create a user in the AWS IAM (Identity and Access Management) for the write access to the repository.
6. Install s3cmd and configure it (`s3cmd --configure`). Enter the access key and the secret key of the user you have just created.
7. Configure the new repository in the *repman* configuration file.

### Google Cloud Storage

Some hints to configure the Google Cloud Storage prior to use it with repman:

1. Create a Google account
2. Create a project and a bucket with a folder structure of your desire to host the repository.
3. Make sure that the folders that contain the repository data are publicly readable.
4. Configure the write access
5. Install Google Cloud CLI on your local machine and initialize it (`gcloud init`)
6. Configure the new repository in the repman configuration file.

Note: repman uses the command `gsutil` (part of [google-cloud-cli](https://aur.archlinux.org/packages/google-cloud-cli)) to transfer data between the local file system and Google Cloud Storage. Make sure to switch to the correct configuration with the command `gcloud` before running repman.

## Troubeshooting

### When installing repman from AUR with an AUR helper, it complains that dependencies cannot be installed

repman is available for different architectures, and it has different dependencies for such architectures. Thus, repmans PKGBUILD contains declarations for architecture-specific dependencies. Unfortunately, some AUR helpers cannot handle such declarations properly and thus complain about a missing dependency which is irrelevant for the current architecture. To solve this, either use an AUR helper that supports architecture-specific dependencies (such as [paru](https://github.com/morganamilo/paru) or [trizen](https://github.com/trizen/trizen)) or download a snapshot of the repman package from AUR and install the package with makepkg.

### repman cannot add `...-debug` packages to a repository

When building a package (via `repman add` or `repman update`), the error message

    ==> ERROR: Package "<{SOME_PATH}.cache/repman/tmp/{SOME_PID}/pkg/{PACKAGE_NAME}-debug-<...>.pkg.tar.zst" was not built and thus not added to the repository

is displayed.

Reason is that because of the configuration of makepkg, in a addition to a 'regular' package, a corresponding package with debugging information (package name: `<NAME_OF_REGULAR PACKAGE>-debug`) is supposed to be built, but this package was not built for some reason.

In order to avoid such error messages, adjust the `makepkg.conf` file that is used for building packages (`/etc/makepkg.conf`, for example - or a corresponding file that is either specific to *repman* or a repository - see the *FILES AND DIRECTORIES* chapter). In that file, replace the option `debug` by `!debug`. After that, the system will no longer try building such debugging packages. For further information, see https://man.archlinux.org/man/makepkg.conf.5.en.html.

If that is an AUR package, you should inform the package maintainer. Maybe the PKGBUILD file must be adjusted.

### repman adds packages of name {PACKAGE_NAME}-debug to a repository

See the topic above. Such packages are created because of the configuration in `makepkg.conf`. Adjust it as described above.

### repman displays weird error messages

Under the hood, repman uses tools like makepkg or makechrootpkg. In case of errors, repman displays the error messages of these tools, and sometimes these messages are not easy to understand. Often, errors are raised by makepkg or makechrootpkg because a dependency is not declared in the PKGBUILD of a package (see below). Please read the error messages carefully and try to figure out if the root cause could be a missing dependency. If that is the case, install it as described below.  

### Adding packages to a remote repository leads to an rsync error

This error can happen, if the remote location is an [SSH](https://en.wikipedia.org/wiki/Secure_Shell)-accessible server and in the PKGBUILD file of the package the attribute `epoch` is set. In this case, the name of the package contains a colon. If the system that hosts the remote repository does not allow colons in file names, rsync throws an error.
  
Unfortunately, there's no other solution than either changing the remote system / hoster or to not add such a packages to the repository. Since inconsistencies can occur (after such an error occurred, the repository database can, for example, contain a package but the corresponding package tarball is not stored), `repman cleanup` helps to make the repository consistent again.

### The build process stops because a dependency is missing

Normally, this happens because a dependency is not maintained in the PKGBUILD script of the package. Often, for example, package maintainers do not list git as make dependency, but since repman does the build in a chroot container, git is not installed there by default. 

If you are the owner of the package you want to build/add, adjust the corresponding PKGBUILD file and run `repman add` again.
    
Otherwise, if the dependency is in the official Arch Linux repositories, add it to the chroot container that is used for your repository by executing as root:

    $ pacstrap ~/.cache/repman/chroots/<REPOSITORY> <DEPENDENCY>

Run `repman add` again.

Otherwise, if the dependency is an AUR package, add it to your repo via `repman add`. Now, try to add the other package again via `repman add`.

### During the build process, the error "error: PACKAGE: signature from NAME, MAIL ADDRESS is invalid" occurs

This is caused by a package or a dependency that is signed but the key is not known to [gpg](https://en.wikipedia.org/wiki/GNU_Privacy_Guard). The problem can be solved by making the key known:

    $ gpg --recv-keys <KEY-ID>

If the maintainer of the package did not put the key into the `validpgpkeys` array in the PKGBUILD file, the key must also be signed locally to indicate that you trust it:

    $ gpg --lsign-key <KEY-ID>

Run `repman add` again.

### A package of a remote repository requires dependencies from another custom repository

Add the custom repository to the `pacman.conf` file that repman uses for the remote repository:

    ~/.config/repman/pacman-<REPOSITORY>.conf

### The chroot container of a repository cannot be deleted since it contains read-only file systems

This situation can occur if the creation of the chroot container was interrupted by the user.

Type

    $ mount
    
to find out the file systems and their mount points. Then, unmount each file system with 

    $ umount <MOUNT-POINT>

Now you should be able to remove the chroot container.

# Implementation details

repman utilizes tools like [makechrootpkg](https://wiki.archlinux.org/index.php/DeveloperWiki:Building_in_a_clean_chroot), [makepkg](https://www.archlinux.org/pacman/makepkg.8.html), [repo-add](https://www.archlinux.org/pacman/repo-add.8.html), and repo-remove. [rsync](https://wiki.archlinux.org/index.php/Rsync), or vendor-specific tools such as [s3cmd](https://github.com/s3tools/s3cmd) or [gsutil](https://cloud.google.com/storage/docs/gsutil) are used to
transfer repositories between remote locations and the local file system. The local copies are manipulated with the above-mentioned tools.

# License

[GNU Public License v3.0](https://gitlab.com/mipimipi/repman/blob/main/LICENSE)
