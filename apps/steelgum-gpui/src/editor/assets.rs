use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, Result, ensure};
use gpui::{
    AnyElement, Context, Image, ImageFormat, ObjectFit, SharedString, div, img, prelude::*, px, rgb,
};
use image::{ImageReader, Limits};

use steelgum::project::{ProjectManifest, resolve_project_path};

use super::workspace::{
    ACCENT, SELECTED, SHELL, create_action, matches_entry, note, sidebar_action,
};
use super::{Project, ReviewWindow, Tool};

#[derive(Default)]
pub(super) struct AssetBrowser {
    generation: u64,
    scanning: bool,
    importing: bool,
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
        let previous_selection = self
            .assets
            .selected
            .take()
            .filter(|_| self.tool == Some(Tool::Assets));
        self.assets = AssetBrowser {
            generation: self.assets.generation.wrapping_add(1),
            importing: self.assets.importing,
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
                        let selected =
                            previous_selection.filter(|path| listing.files.contains(path));
                        view.assets.listing = Some(listing);
                        if let Some(path) = selected {
                            view.preview_asset(path, cx);
                        } else if view.tool == Some(Tool::Assets) {
                            view.close_properties();
                        }
                    }
                    Err(error) => view.assets.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn preview_asset(&mut self, path: PathBuf, cx: &mut Context<Self>) {
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

    pub(super) fn asset_paths(&self) -> &[PathBuf] {
        self.assets
            .listing
            .as_ref()
            .map_or(&[], |listing| &listing.files)
    }

    pub(super) fn load_assets_for_search(&mut self, cx: &mut Context<Self>) {
        if self.assets.listing.is_none()
            && !self.assets.scanning
            && !self.assets.importing
            && self.assets.error.is_none()
            && self.project.assets_root().is_some()
        {
            self.refresh_assets(cx);
        }
    }

    pub(super) fn assets_navigation(
        &self,
        max_height: gpui::Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let assets = &self.assets;
        let query = self.child_query(Tool::Assets);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .ml_3()
            .when(self.project.source.is_some(), |pane| {
                pane.child(
                    create_action("import-asset", "+ Import Asset")
                        .tab_stop(self.menu_open)
                        .on_click(cx.listener(|view, _, _, cx| view.import_asset(cx))),
                )
            })
            .child(
                sidebar_action("refresh-assets", "Refresh", false)
                    .tab_stop(self.menu_open)
                    .on_click(cx.listener(|view, _, _, cx| view.refresh_assets(cx))),
            )
            .when(assets.importing, |pane| {
                pane.child(note("Importing asset…"))
            })
            .child(
                div()
                    .id("asset-files")
                    .max_h(max_height)
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_1()
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
                        .children(
                            listing
                                .files
                                .iter()
                                .filter(|path| matches_entry(&path.to_string_lossy(), "", &query))
                                .map(|path| {
                                    let id: SharedString =
                                        format!("asset-{}", path.display()).into();
                                    let active = self.tool == Some(Tool::Assets)
                                        && assets.selected.as_ref() == Some(path);
                                    let path = path.clone();
                                    div()
                                        .id(id)
                                        .tab_index(0)
                                        .tab_stop(self.menu_open)
                                        .role(gpui::accesskit::Role::Button)
                                        .aria_toggled(if active {
                                            gpui::accesskit::Toggled::True
                                        } else {
                                            gpui::accesskit::Toggled::False
                                        })
                                        .when(active, |row| {
                                            row.bg(rgb(SELECTED)).text_color(rgb(ACCENT))
                                        })
                                        .hover(|style| style.text_color(rgb(ACCENT)))
                                        .focus_visible(|style| style.underline())
                                        .rounded_md()
                                        .py_1()
                                        .px_2()
                                        .text_sm()
                                        .truncate()
                                        .cursor_pointer()
                                        .child(path.display().to_string())
                                        .on_click(cx.listener(move |view, _, _, cx| {
                                            view.show_tool(Tool::Assets);
                                            view.preview_asset(path.clone(), cx);
                                        }))
                                }),
                        )
                    }),
            )
            .into_any_element()
    }

    pub(super) fn asset_properties(&self) -> AnyElement {
        let Some(path) = self.assets.selected.as_deref() else {
            return div().into_any_element();
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_sm().child(path.display().to_string()))
            .child(self.assets.preview_panel())
            .into_any_element()
    }

    pub(super) fn selected_asset(&self) -> Option<(&Path, &Path)> {
        Some((
            self.assets.listing.as_ref()?.directory.as_path(),
            self.assets.selected.as_deref()?,
        ))
    }

    pub(super) fn importing_asset(&self) -> bool {
        self.assets.importing
    }

    fn import_asset(&mut self, cx: &mut Context<Self>) {
        if self.loading || self.assets.importing {
            return;
        }
        let Some((root, scene)) = self.project.source.clone() else {
            return;
        };
        let picker = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import an asset".into()),
        });
        self.assets.importing = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = match picker.await {
                Ok(Ok(Some(mut paths))) => match paths.pop() {
                    Some(path) => {
                        let repo = root.clone();
                        cx.background_executor()
                            .spawn(async move {
                                let name = copy_asset(&repo, &path)?;
                                let directory = asset_directory(&repo)?;
                                Ok::<_, anyhow::Error>((
                                    name,
                                    AssetListing::read(&repo, &directory)?,
                                ))
                            })
                            .await
                            .map(Some)
                    }
                    None => Ok(None),
                },
                Ok(Ok(None)) => Ok(None),
                Ok(Err(error)) => Err(error),
                Err(error) => Err(error.into()),
            };
            let _ = this.update(cx, |view, cx| {
                if view.project.source.as_ref() != Some(&(root, scene)) {
                    return;
                }
                view.assets.importing = false;
                match result {
                    Ok(Some((path, listing))) => {
                        view.assets.listing = Some(listing);
                        if view.expanded_tool == Some(Tool::Assets) {
                            view.show_tool(Tool::Assets);
                            view.preview_asset(path, cx);
                        }
                    }
                    Ok(None) => {}
                    Err(error) => view.assets.error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
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

fn asset_directory(repo: &Path) -> Result<PathBuf> {
    let relative = ProjectManifest::discover(repo)?
        .and_then(|project| project.assets)
        .unwrap_or_else(|| PathBuf::from("assets"));
    let root = repo.canonicalize()?;
    let mut directory = root.clone();
    for component in relative.components() {
        directory.push(component);
        if !directory.exists() {
            match fs::create_dir(&directory) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        directory = directory.canonicalize()?;
        ensure!(
            directory.starts_with(&root),
            "The assets folder resolves outside the project."
        );
    }
    Ok(resolve_project_path(repo, &relative)?)
}

fn copy_asset(repo: &Path, source: &Path) -> Result<PathBuf> {
    let name = source.file_name().context("The asset needs a file name")?;
    ensure!(
        !name.to_string_lossy().starts_with('.'),
        "Choose a file with a visible name."
    );
    ensure!(source.metadata()?.is_file(), "Choose an asset file.");
    let directory = asset_directory(repo)?;
    let mut file = fs::File::open(source)?;
    let mut incoming = tempfile::NamedTempFile::new_in(&directory)?;
    std::io::copy(&mut file, &mut incoming)?;
    incoming.as_file().sync_all()?;
    incoming
        .persist_noclobber(directory.join(name))
        .with_context(|| format!("Could not import {}", source.display()))?;
    Ok(PathBuf::from(name))
}

pub(super) fn remove_asset(repo: &Path, directory: &Path, relative: &Path) -> Result<()> {
    let requested = resolve_project_path(directory, relative)?;
    let root = repo.canonicalize()?;
    ensure!(
        requested.starts_with(root),
        "The asset resolves outside the project."
    );
    let path = directory.join(relative);
    ensure!(
        fs::symlink_metadata(&path)?.is_file(),
        "The asset is no longer a regular file."
    );
    fs::remove_file(&path).with_context(|| format!("Could not remove {}", relative.display()))
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
    fn imports_and_removes_a_file_without_overwriting_an_existing_asset() -> Result<()> {
        let source = tempfile::tempdir()?;
        let project = tempfile::tempdir()?;
        let file = source.path().join("mesh.txt");
        fs::write(&file, "original")?;
        let path = copy_asset(project.path(), &file)?;
        let assets = project.path().join("assets");
        fs::write(&file, "replacement")?;
        assert!(copy_asset(project.path(), &file).is_err());
        assert_eq!(fs::read_to_string(assets.join(&path))?, "original");
        remove_asset(project.path(), &assets, &path)?;
        assert!(!assets.join(&path).exists());
        assert!(file.exists());
        Ok(())
    }

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
