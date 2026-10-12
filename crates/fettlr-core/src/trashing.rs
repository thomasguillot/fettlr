use std::path::Path;

use crate::cli_records::CoreError;

pub trait Trashing: Send + Sync {
    fn trash(&self, path: &Path) -> Result<(), CoreError>;
}

#[derive(Debug, Default)]
pub struct SystemTrasher;

impl Trashing for SystemTrasher {
    fn trash(&self, path: &Path) -> Result<(), CoreError> {
        let mut context = trash::TrashContext::default();
        #[cfg(target_os = "macos")]
        {
            use trash::macos::{DeleteMethod, TrashContextExtMacos};
            context.set_delete_method(DeleteMethod::NsFileManager);
        }
        context.delete(path).map_err(|error| CoreError::Trash {
            path: path.display().to_string(),
            reason: error.to_string(),
        })
    }
}
