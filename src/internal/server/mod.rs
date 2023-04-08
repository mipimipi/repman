pub mod file;
pub mod rsync;

use std::path::Path;

pub trait Server: Sync {
    fn is_remote(&self) -> bool {
        false
    }

    fn download_repo(&self, _local_dir: &Path) -> anyhow::Result<()> {
        Ok(())
    }
    fn upload_repo(&self, _local_dir: &Path) -> anyhow::Result<()> {
        Ok(())
    }
}
