use crate::internal::{cfg, repo::Repo};
use anyhow::{anyhow, Context};
use arch_msgs::*;
use clap::Parser;
use dialoguer::Confirm;

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
            ignore_arch,
            sign,
        } => {
            if *no_chroot && *clean_chroot {
                return Err(anyhow!(
                    "If '-n/--nochroot' is set, setting '-c/--clean' does not make sense"
                ));
            }

            Repo::new(repo_name)?.add(
                aur_pkg_names,
                pkgbuild_dirs,
                *no_chroot,
                *ignore_arch,
                *clean_chroot,
                *sign,
            )
        }

        // Cleanup a repository
        cli::Commands::CleanUp { repo_name } => Repo::new(repo_name)
            .with_context(|| format!("Cannot clear data of repository {}", repo_name))?
            .clean_up(),

        // Delete local data of a repository - i.e., chroot directory and/or
        // local repository directory in case of a remote repository
        cli::Commands::Clear {
            repo_name,
            clear_cache,
            clear_chroot,
        } => {
            let repo = Repo::new(repo_name)
                .with_context(|| format!("Cannot clear data of repository {}", repo_name))?;
            if *clear_cache {
                if !repo.is_remote() {
                    warning!(
                "Repository {} is local. Thus, removing its cache directory does not makes sense",
                repo_name
            );
                } else {
                    repo.remove_cache_dir().with_context(|| {
                        format!("Cannot remove cache directory of repository {}", repo_name)
                    })?;
                    msg!("Cache directory of repository {} removed", repo_name);
                }
            }
            if *clear_chroot {
                repo.remove_chroot_dir().with_context(|| {
                    format!("Cannot remove chroot directory of repository {}", repo_name)
                })?;
                msg!("Chroot directory of repository {} removed", repo_name);
            }
            Ok(())
        }

        // List packages of one repository
        cli::Commands::Ls { repo_name } => {
            let err_msg = format!("Cannot list content of repository {}", repo_name);
            Repo::new(repo_name)
                .with_context(|| err_msg.clone())?
                .list()
                .with_context(|| err_msg)
        }

        // List all configured repositories
        cli::Commands::LsRepos => {
            for repo_name in cfg::repos()?.keys() {
                println!("{}", repo_name)
            }
            Ok(())
        }

        // Create chroot container for a repository
        cli::Commands::MkChroot { repo_name } => {
            let err_msg = format!("Cannot make chroot container for repository {}", repo_name);
            let repo = Repo::new(repo_name).with_context(|| err_msg.clone())?;
            if repo.chroot_exists() {
                if Confirm::new()
                    .with_prompt(format!(
                        "A chroot for repository {} exists already. It is now being deleted. OK?",
                        repo_name
                    ))
                    .default(true)
                    .interact()
                    .with_context(|| err_msg.clone())?
                {
                    repo.remove_chroot_dir().with_context(|| err_msg.clone())?
                } else {
                    return Ok(());
                }
            }
            repo.make_chroot().with_context(|| err_msg)?;
            Ok(())
        }

        // Remove packages of a repository
        cli::Commands::Rm {
            repo_name,
            no_confirm,
            pkg_names,
        } => {
            if pkg_names.is_empty() {
                Ok(())
            } else {
                let err_msg = format!("Cannot remove packages from repository {}", &repo_name);
                Repo::new(repo_name)
                    .with_context(|| err_msg.clone())?
                    .remove(pkg_names, *no_confirm)
                    .with_context(|| err_msg)
            }
        }

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
            _ => {
                let err_msg = format!("Cannot sign packages of repository {}", repo_name);
                Repo::new(repo_name)
                    .with_context(|| err_msg.clone())?
                    .sign(if *all { None } else { Some(pkg_names) })
                    .with_context(|| err_msg)
            }
        },

        // Update packages
        cli::Commands::Update {
            repo_name,
            clean_chroot,
            no_chroot,
            ignore_arch,
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
                false if pkg_names.is_empty() => {
                    warning!("Either submit package names or set option '--all'");
                    Ok(())
                }
                _ => Repo::new(repo_name)?.update(
                    if *all { None } else { Some(pkg_names) },
                    *no_chroot,
                    *ignore_arch,
                    *clean_chroot,
                    *no_confirm,
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
