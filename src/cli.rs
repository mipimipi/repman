//! Definition of the command line interface of repman

use clap::{Parser, Subcommand};
use indoc::indoc;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = env!("CARGO_PKG_NAME"),
    version = env!("CARGO_PKG_VERSION"),
    propagate_version = true,
    author = env!("CARGO_PKG_AUTHORS"),
    about = env!("CARGO_PKG_DESCRIPTION"),
    long_about = indoc! {"
    repman (Custom Repository Management) 
    Copyright (C) 2019-2022 Michael Picht <https://gitlab.com/mipimipi/repman>
    
    repman helps to manage custom repositories for Arch Linux packages.
    "}
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(
        name = "add",
        about = "Build and add packages to a repository",
        long_about = indoc! {"
            Build and add packages to a repository that can either be from the AUR or from
            PKGBUILD files that are stored in the local file system. The packages can be
            signed. For this, the environment variable GPGKEY must contain the id of the
            corresponding gpg key
        "}
    )]
    Add {
        #[arg(short = 'r', long = "repo", help = "Repository")]
        repo_name: String,
        #[arg(short = 'a', long = "aur", action = clap::ArgAction::Append, help = "Name of AUR package")]
        aur_pkg_names: Vec<String>,
        #[arg(short = 'd', long = "directory", action = clap::ArgAction::Append, help = "Local directory with PKGBUILD file")]
        pkgbuild_dirs: Vec<PathBuf>,
        #[arg(
            short = 'n',
            long = "nochroot",
            help = "Don't build packages in chroot environment"
        )]
        no_chroot: bool,
        #[arg(
            short = 'c',
            long = "clean",
            help = "Remove chroot environment after build"
        )]
        clean_chroot: bool,
        #[arg(short = 's', long = "sign", help = "Sign packages")]
        sign: bool,
    },

    #[command(
        name = "cleanup",
        about = "Clean up a repository",
        long_about = indoc! {"
           To make sure that the repository DB and the package files are consistent to each
           other, it is checked that all package files belong to package (versions) that
           are contained in the repository DB.
           It is also checked that all signature files fit to their counterpart files.
        "}
    )]
    CleanUp {
        #[arg(short = 'r', long = "repo", help = "Repository")]
        repo_name: String,
    },

    #[command(
        name = "clear",
        about = "Delete local data of a repository",
        long_about = indoc! {"
            Delete the chroot container and/or the local copy/cache of a repository
        "}
    )]
    Clear {
        #[arg(short = 'r', long = "repo", help = "Repository")]
        repo_name: String,
        #[arg(long = "cache", help = "Delete local copy of a remote repository")]
        clear_cache: bool,
        #[arg(long = "chroot", help = "Delete chroot container of a repository")]
        clear_chroot: bool,
    },

    #[command(
        name = "ls",
        about = "List packages of a repository",
        long_about = indoc! {"
            List the packages of a repository with their architectures, versions, if they are
            signed and if other packages depend on them. It is also indicated whether the
            repository DB is signed    
        "}
    )]
    Ls {
        #[arg(short = 'r', long = "repo", help = "Repository")]
        repo_name: String,
    },

    #[command(
        name = "lsrepos",
        about = "List all repositories",
        long_about = indoc! {"
            List all repositories that are defined in the configuration file
        "}
    )]
    LsRepos,

    #[command(
        name = "mkchroot",
        about = "Create a chroot container for a repository",
        long_about = indoc! {"
            Create a chroot container to be used for building packages for a repository
        "}
    )]
    MkChroot {
        #[arg(short = 'r', long = "repo", help = "Repository")]
        repo_name: String,
    },

    #[command(
        name = "rm",
        about = "Remove packages from a repository",
        long_about = indoc! {"
            Packages are removed from the repository DB, and all related package files are
            deleted. This includes all existing signature files.
        "}
    )]
    Rm {
        #[arg(short = 'r', long = "repo", help = "Repository")]
        repo_name: String,
        #[arg(
            long = "noconfirm",
            help = "Don't ask for confirmation and remove packages directly"
        )]
        no_confirm: bool,
        pkg_names: Vec<String>,
    },

    #[command(
        name = "sign",
        about = "Sign packages of a repository",
        long_about = indoc! {"
            Signs either all or only specific packages of a repository. The repository DB is
            signed as well if that is required by the configuration.
        "}
    )]
    Sign {
        #[arg(short = 'r', long = "repo", help = "Repository")]
        repo_name: String,
        #[arg(long, help = "All packages")]
        all: bool,
        pkg_names: Vec<String>,
    },

    #[command(
        name = "update",
        about = "Update AUR packages of a repository",
        long_about = indoc! {"
            Updates either all or only specific AUR packages of a repository. The packages
            for which a newer version exist in AUR are determined, new package versions are
            built and replace the current versions in the repository.
            If the old package version was signed, the new version will also be signed.
            Therefore, the environment variable GPGKEY must contain the id of the
            corresponding gpg key.
        "}
    )]
    Update {
        #[arg(short = 'r', long = "repo", help = "Repository")]
        repo_name: String,
        #[arg(
            short = 'n',
            long = "nochroot",
            help = "Don't build packages in chroot environment"
        )]
        no_chroot: bool,
        #[arg(
            short = 'c',
            long = "clean",
            help = "Remove chroot environment after build"
        )]
        clean_chroot: bool,
        #[arg(
            long = "noconfirm",
            help = "Don't ask for confirmation and update packages directly"
        )]
        no_confirm: bool,
        #[arg(long, help = "All packages")]
        all: bool,
        pkg_names: Vec<String>,
    },
}
