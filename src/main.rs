use crate::internal::api;
use anyhow::anyhow;
use arch_msgs::*;
use clap::Parser;

mod cli;
mod internal;

/// Executes repman (sub) command by calling the corresponding function from their
/// internal API
fn execute(args: &cli::Args) -> anyhow::Result<()> {
    match &args.command {
        // Build and add packages
        cli::Commands::Add {
            repo_name,
            aur_pkg_names,
            pkgbuild_dirs,
            clean_chroot,
            no_chroot,
            sign,
        } => {
            if *no_chroot && *clean_chroot {
                return Err(anyhow!(
                    "If '-n/--nochroot' is set, setting '-c/--clean' does not make sense"
                ));
            }

            api::add(
                repo_name,
                aur_pkg_names,
                pkgbuild_dirs,
                *no_chroot,
                *clean_chroot,
                *sign,
            )
        }

        // Cleanup a repository
        cli::Commands::CleanUp { repo_name } => api::clean_up(repo_name),

        // Delete local data of a repository - i.e., chroot directory and/or
        // local repository directory in case of a remote repository
        cli::Commands::Clear {
            repo_name,
            clear_cache,
            clear_chroot,
        } => api::clear(repo_name, *clear_cache, *clear_chroot),

        // List packages of one repository
        cli::Commands::Ls { repo_name } => api::ls(repo_name),

        // List all configured repositories
        cli::Commands::LsRepos => api::ls_repos(),

        // Create chroot container for a repository
        cli::Commands::MkChroot { repo_name } => api::mkchroot(repo_name),

        // Remove packages of a repository
        cli::Commands::Rm {
            repo_name,
            no_confirm,
            pkg_names,
        } => api::rm(repo_name, *no_confirm, pkg_names),

        // Sign packages of a repository
        cli::Commands::Sign {
            repo_name,
            all,
            pkg_names,
        } => match *all {
            true if !pkg_names.is_empty() => Err(anyhow!(
                "Either submit package names or set option '--all', but not both."
            )),
            false if pkg_names.is_empty() => Ok(()),
            _ => api::sign(repo_name, if *all { None } else { Some(pkg_names) }),
        },
        // Update packages
        cli::Commands::Update {
            repo_name,
            clean_chroot,
            no_chroot,
            no_confirm,
            all,
            pkg_names,
        } => {
            if *no_chroot && *clean_chroot {
                return Err(anyhow!(
                    "If '-n/--nochroot' is set, setting '-c/--clean' does not make sense"
                ));
            }

            match *all {
                true if !pkg_names.is_empty() => Err(anyhow!(
                    "Either submit package names or set option '--all', but not both."
                )),
                false if pkg_names.is_empty() => Ok(()),
                _ => api::update(
                    repo_name,
                    *no_chroot,
                    *clean_chroot,
                    *no_confirm,
                    if *all { None } else { Some(pkg_names) },
                ),
            }
        }
    }
}

fn main() {
    // Execute repman (sub) command. In case of an error: Exit with error code
    if let Err(err) = execute(&cli::Args::parse()) {
        error!("{:?}", err);
        std::process::exit(1);
    }
}
