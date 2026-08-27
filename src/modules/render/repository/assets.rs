// Staging for a render's downloaded `logoUrl` image. Typst resolves
// `image("foo.png")` relative to the `.typ` that calls it, so the fetched
// bytes are written into that template's own directory under a unique
// `.rimg_<nanoid>.<ext>` name, and the returned `StagedImage` deletes the
// file when it drops — i.e. as soon as the render that needed it returns,
// success or error. Nothing about the image is persisted.
//
// Concurrency: the name is per-request unique, so simultaneous renders never
// collide. A crash between `write` and `drop` leaves an orphan, which
// `RenderEngine::warm_up`'s sweep clears on the next start.

use std::path::{Path, PathBuf};

use nanoid::nanoid;

use crate::clients::{http::FetchedImage, render::REMOTE_IMAGE_PREFIX};

/// A downloaded image written next to a template for the duration of one
/// compile. Drop removes the file.
pub(crate) struct StagedImage {
    /// The bare filename to put in the render payload's `logo` field.
    pub(crate) file_name: String,
    full_path: PathBuf,
}

impl StagedImage {
    /// Writes `image` into the directory of `template_rel_path` (relative to
    /// `templates_dir`, e.g. `documents/doc_temp_x.typ` → `templates/documents/`).
    pub(crate) fn write(
        templates_dir: &Path,
        template_rel_path: &str,
        image: &FetchedImage,
    ) -> std::io::Result<Self> {
        let dir = match template_rel_path.rsplit_once('/') {
            Some((parent, _)) => templates_dir.join(parent),
            None => templates_dir.to_path_buf(),
        };
        let file_name = format!("{}{}.{}", REMOTE_IMAGE_PREFIX, nanoid!(16), image.extension);
        let full_path = dir.join(&file_name);
        std::fs::write(&full_path, &image.bytes)?;
        Ok(Self {
            file_name,
            full_path,
        })
    }
}

impl Drop for StagedImage {
    fn drop(&mut self) {
        if let Err(err) = std::fs::remove_file(&self.full_path) {
            tracing::warn!(
                path = %self.full_path.display(),
                %err,
                "failed to delete staged remote image; warm_up's sweep will retry"
            );
        }
    }
}
