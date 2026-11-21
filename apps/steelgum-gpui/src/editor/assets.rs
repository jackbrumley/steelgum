use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, Result, ensure};
use gpui::{
    AnyElement, Context, Image, ImageFormat, ObjectFit, SharedString, div, img, prelude::*, px, rgb,
};
use image::{ImageReader, Limits};

use super::workspace::{ACCENT, BORDER, SHELL, action, note};
use super::{Project, ReviewWindow};

#[derive(Default)]
pub(super) struct AssetBrowser {
    generation: u64,
    scanning: bool,
    listing: Option<AssetListing>,
    selected: Option<PathBuf>,
    preview: Option<Result<Preview, String>>,
    error: Option<String>,
}

struct AssetListing {
    directory: PathBuf,
    files: Vec<PathBuf>,
}

enum Preview {
    Image {
        thumbnail: Arc<Image>,
        size: (u32, u32),
    },
    File {
        bytes: u64,
    },
}

impl Project {
    fn assets_root(&self) -> Option<PathBuf> {
        let (repo, _) = self.source.as_ref()?;
        if let Some(relative) = self
            .manifest
            .as_ref()
            .and_then(|manifest| manifest.assets.as_ref())
        {
            Some(repo.join(relative))
        } else {
            let directory = repo.join("assets");
            directory.is_dir().then_some(directory)
        }
    }
}

impl ReviewWindow {
    pub(super) fn refresh_assets(&mut self, cx: &mut Context<Self>) {
        let previous_selection = self.assets.selected.take();
        self.assets = AssetBrowser {
            generation: self.assets.generation.wrapping_add(1),
            ..AssetBrowser::default()
        };
        let (Some(source), Some(directory)) =
            (self.project.source.clone(), self.project.assets_root())
        else {
            cx.notify();
            return;
        };
        let generation = self.assets.generation;
        self.assets.scanning = true;
        cx.notify();
        cx.spawn(async move |entity, cx| {
            let repo = source.0.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    AssetListing::read(&repo, &directory).map_err(|error| format!("{error:#}"))
                })
                .await;
            let _ = entity.update(cx, |view, cx| {
                if view.assets.generation != generation
                    || view.project.source.as_ref() != Some(&source)
                {
                    return;
                }
                view.assets.scanning = false;
                match result {
                    Ok(listing) => {
                        let selected = previous_selection
                            .filter(|path| listing.files.contains(path))
                            .or_else(|| listing.files.first().cloned());
                        view.assets.listing = Some(listing);
                        if let Some(path) = selected {
                            view.preview_asset(path, cx);
                        }
                    }
                    Err(error) => view.assets.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn preview_asset(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let (Some(source), Some(listing)) = (self.project.source.clone(), &self.assets.listing)
        else {
            return;
        };
        let directory = listing.directory.clone();
        self.assets.generation = self.assets.generation.wrapping_add(1);
        let generation = self.assets.generation;
        self.assets.selected = Some(path.clone());
        self.assets.preview = None;
        cx.notify();
        cx.spawn(async move |entity, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    Preview::read(&directory, &path).map_err(|error| format!("{error:#}"))
                })
                .await;
            let _ = entity.update(cx, |view, cx| {
                if view.assets.generation == generation
                    && view.project.source.as_ref() == Some(&source)
                {
                    view.assets.preview = Some(result);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn assets_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let assets = &self.assets;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                action("refresh-assets", "Refresh assets", false)
                    .on_click(cx.listener(|view, _, _, cx| view.refresh_assets(cx))),
            )
            .when(assets.scanning, |pane| pane.child(note("Loading assets…")))
            .when_some(assets.error.as_ref(), |pane, error| {
                pane.child(note(error.clone()))
            })
            .when(
                !assets.scanning && assets.error.is_none() && assets.listing.is_none(),
                |pane| {
                    pane.child(note(
                        "Open a game with an assets folder to browse its files.",
                    ))
                },
            )
            .when_some(assets.listing.as_ref(), |pane, listing| {
                pane.when(listing.files.is_empty(), |pane| {
                    pane.child(note("No asset files found."))
                })
                .when(assets.selected.is_some(), |pane| {
                    pane.child(assets.preview_panel())
                })
                .children(listing.files.iter().map(|path| {
                    let id: SharedString = format!("asset-{}", path.display()).into();
                    let active = assets.selected.as_ref() == Some(path);
                    let path = path.clone();
                    div()
                        .id(id)
                        .tab_stop(true)
                        .role(gpui::accesskit::Role::Button)
                        .aria_toggled(if active {
                            gpui::accesskit::Toggled::True
                        } else {
                            gpui::accesskit::Toggled::False
                        })
                        .border_1()
                        .border_color(rgb(if active { ACCENT } else { BORDER }))
                        .bg(rgb(SHELL))
                        .rounded_md()
                        .py_1()
                        .px_2()
                        .text_sm()
                        .truncate()
                        .cursor_pointer()
                        .child(path.display().to_string())
                        .on_click(
                            cx.listener(move |view, _, _, cx| view.preview_asset(path.clone(), cx)),
                        )
                }))
            })
            .into_any_element()
    }
}

impl AssetBrowser {
    fn preview_panel(&self) -> AnyElement {
        match &self.preview {
            None => note("Loading preview…").into_any_element(),
            Some(Err(error)) => note(error.clone()).into_any_element(),
            Some(Ok(Preview::File { bytes })) => note(format!(
                "{bytes} bytes · Image previews support PNG and JPEG."
            ))
            .into_any_element(),
            Some(Ok(Preview::Image {
                thumbnail,
                size: (width, height),
            })) => div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div().bg(rgb(SHELL)).rounded_md().child(
                        img(thumbnail.clone())
                            .w_full()
                            .h(px(132.))
                            .object_fit(ObjectFit::Contain),
                    ),
                )
                .child(note(format!("{width} × {height} px")))
                .into_any_element(),
        }
    }
}

impl AssetListing {
    fn read(repo: &Path, directory: &Path) -> Result<Self> {
        let repo = repo
            .canonicalize()
            .context("Could not open the project folder")?;
        let directory = directory
            .canonicalize()
            .with_context(|| format!("Could not open assets folder {}", directory.display()))?;
        ensure!(
            directory.starts_with(&repo),
            "The assets folder resolves outside the project."
        );
        let mut files = Vec::new();
        list_files(&directory, &directory, &mut files)?;
        files.sort_unstable();
        Ok(Self { directory, files })
    }
}

fn list_files(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)
        .with_context(|| format!("Could not list assets in {}", directory.display()))?
    {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let kind = entry.file_type()?;
        let path = entry.path();
        if kind.is_dir() {
            list_files(root, &path, files)?;
        } else if kind.is_file() {
            files.push(path.strip_prefix(root)?.to_owned());
        }
    }
    Ok(())
}

impl Preview {
    fn read(directory: &Path, relative: &Path) -> Result<Self> {
        let path = directory
            .join(relative)
            .canonicalize()
            .with_context(|| format!("Could not open asset {}", relative.display()))?;
        ensure!(
            path.starts_with(directory),
            "The selected asset resolves outside its folder."
        );

        let format = relative
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| match ext.to_ascii_lowercase().as_str() {
                "png" => Some(image::ImageFormat::Png),
                "jpg" | "jpeg" => Some(image::ImageFormat::Jpeg),
                _ => None,
            });
        let Some(format) = format else {
            return Ok(Self::File {
                bytes: path.metadata()?.len(),
            });
        };

        let mut reader = ImageReader::open(&path)?;
        reader.set_format(format);
        let mut limits = Limits::default();
        limits.max_image_width = Some(16_384);
        limits.max_image_height = Some(16_384);
        reader.limits(limits);
        let image = reader
            .decode()
            .with_context(|| format!("Could not decode image {}", relative.display()))?;
        let size = (image.width(), image.height());
        let mut png = Cursor::new(Vec::new());
        image
            .thumbnail(512, 512)
            .write_to(&mut png, image::ImageFormat::Png)
            .context("Could not prepare the asset preview")?;
        Ok(Self::Image {
            thumbnail: Arc::new(Image::from_bytes(ImageFormat::Png, png.into_inner())),
            size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_and_previews_nested_project_textures() -> Result<()> {
        let project = tempfile::tempdir()?;
        let directory = project.path().join("assets");
        fs::create_dir_all(directory.join("textures"))?;
        let texture = PathBuf::from("textures/beacon.png");
        image::RgbImage::from_pixel(4, 6, image::Rgb([145, 139, 228]))
            .save(directory.join(&texture))?;

        let listing = AssetListing::read(project.path(), &directory)?;
        assert_eq!(listing.files.as_slice(), std::slice::from_ref(&texture));
        let preview = Preview::read(&listing.directory, &texture)?;
        assert!(matches!(preview, Preview::Image { size: (4, 6), .. }));
        fs::write(project.path().join("notes.txt"), "private project notes")?;
        assert!(Preview::read(&listing.directory, Path::new("../notes.txt")).is_err());
        assert!(AssetListing::read(&directory.join("textures"), &directory).is_err());
        Ok(())
    }
}
