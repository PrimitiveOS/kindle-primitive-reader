use super::*;

impl Reader {
    pub(super) fn ocr_language_catalog_rows(&self) -> Vec<Item> {
        let Some(assets) = self.ocr_assets.as_ref() else {
            return vec![Item::new("Offline OCR is unavailable", Action::Heading)];
        };
        let root = assets.model_directory();
        let mut models = crate::ocr_model_catalog::models()
            .enumerate()
            .collect::<Vec<_>>();
        models.sort_by_key(|(_, model)| super::ocr_language_name(model.code).to_lowercase());
        models
            .into_iter()
            .map(|(index, model)| {
                let detail = match crate::ocr_model_catalog::installed_state(&root, model) {
                    crate::ocr_model_catalog::InstalledState::Installed
                        if matches!(model.code, "eng" | "vie") =>
                    {
                        "Included with Reader".to_owned()
                    }
                    crate::ocr_model_catalog::InstalledState::Installed => "Installed".to_owned(),
                    crate::ocr_model_catalog::InstalledState::Missing => {
                        format!("Download · {}", model_size(model.bytes))
                    }
                    crate::ocr_model_catalog::InstalledState::Invalid => {
                        "Local model file needs attention".to_owned()
                    }
                };
                Item::new(
                    super::ocr_language_name(model.code),
                    Action::Show(Screen::OcrLanguageCatalogItem(index)),
                )
                .detail(format!("{detail} · {}", model.code))
            })
            .collect()
    }

    pub(super) fn ocr_language_catalog_item_rows(&self, index: usize) -> Vec<Item> {
        let Some(model) = crate::ocr_model_catalog::model(index) else {
            return vec![Item::new("OCR language unavailable", Action::Heading)];
        };
        let Some(assets) = self.ocr_assets.as_ref() else {
            return vec![Item::new("Offline OCR is unavailable", Action::Heading)];
        };
        let source = format!(
            "https://github.com/tesseract-ocr/tessdata_fast/blob/{}/{}.traineddata",
            crate::ocr_model_catalog::SOURCE_REVISION,
            model.code
        );
        let mut rows = vec![
            Item::new("Language", Action::Heading).detail(super::ocr_language_name(model.code)),
            Item::new("Model file", Action::Heading).detail(format!("{}.traineddata", model.code)),
            Item::new("Model size", Action::Heading).detail(model_size(model.bytes)),
            Item::new("License", Action::Heading).detail("Apache License 2.0"),
            Item::new("Source revision", Action::Heading)
                .detail(&crate::ocr_model_catalog::SOURCE_REVISION[..12]),
            Item::new("Model source", Action::Heading).detail(source),
        ];
        match crate::ocr_model_catalog::installed_state(&assets.model_directory(), model) {
            crate::ocr_model_catalog::InstalledState::Installed => rows.push(
                Item::new("Installed", Action::Heading)
                    .detail("Available in the offline OCR language picker"),
            ),
            crate::ocr_model_catalog::InstalledState::Invalid => rows.push(
                Item::new("Model file needs attention", Action::Heading)
                    .detail("Move the existing file before installing this pinned version"),
            ),
            crate::ocr_model_catalog::InstalledState::Missing => {
                if let Some(job) = self.ocr_model_download.as_ref() {
                    if job.model_index() == index {
                        if job.cancel_requested() {
                            rows.push(Item::new("Canceling download…", Action::Heading));
                        } else {
                            rows.push(
                                Item::new("Download progress", Action::Heading).detail(
                                    job.progress_label()
                                        .unwrap_or_else(|| "Connecting to model source…".into()),
                                ),
                            );
                            rows.push(
                                Item::new("Cancel download", Action::CancelOcrLanguageDownload)
                                    .detail("Reader removes its staging files before stopping"),
                            );
                        }
                    } else {
                        rows.push(Item::new(
                            "Another OCR language download is in progress",
                            Action::Heading,
                        ));
                    }
                } else if self.dictionary_download.is_some() {
                    rows.push(Item::new(
                        "A dictionary download is in progress",
                        Action::Heading,
                    ));
                } else {
                    rows.push(
                        Item::new("Download and install", Action::InstallOcrLanguage(index))
                            .detail(
                                "Downloads this model only and saves its Apache license notice",
                            ),
                    );
                }
            }
        }
        rows
    }

    pub(super) fn install_ocr_language(&mut self, index: usize) -> Result<()> {
        let model = crate::ocr_model_catalog::model(index)
            .ok_or("That OCR language is not in the catalog")?;
        if self.ocr_model_download.is_some()
            || self.dictionary_download.is_some()
            || self.dictionary_size_check.is_some()
        {
            return Err("Another Reader package download is already in progress".into());
        }
        let assets = self
            .ocr_assets
            .as_ref()
            .ok_or("The offline OCR engine is unavailable")?;
        if crate::ocr_model_catalog::installed_state(&assets.model_directory(), model)
            != crate::ocr_model_catalog::InstalledState::Missing
        {
            self.notice = Some("This OCR model already exists".into());
            return Ok(());
        }
        self.ocr_model_download = Some(crate::ocr_model_catalog::Job::start(
            index,
            assets.model_directory(),
        )?);
        self.show(Screen::OcrLanguageCatalogItem(index));
        self.notice = Some(format!(
            "Downloading {} · {}",
            super::ocr_language_name(model.code),
            model_size(model.bytes)
        ));
        Ok(())
    }

    pub(super) fn cancel_ocr_language_download(&mut self) {
        let Some(job) = self.ocr_model_download.as_mut() else {
            return;
        };
        if !job.cancel_requested() {
            job.cancel();
            self.notice = Some("Canceling OCR language download…".into());
        }
    }

    pub(super) fn poll_ocr_model_download(&mut self, now: Instant) -> Option<Change> {
        let poll = self.ocr_model_download.as_mut()?.poll(now);
        let result = match poll {
            crate::ocr_model_catalog::Poll::Pending { progress_changed } => {
                return progress_changed.then_some(Change::Partial);
            }
            crate::ocr_model_catalog::Poll::Finished(result) => result,
        };
        let index = self.ocr_model_download.as_ref()?.model_index();
        self.ocr_model_download = None;
        match result {
            Ok(()) => {
                self.ocr_assets = self
                    .ocr_root
                    .parent()
                    .and_then(Path::parent)
                    .and_then(OcrAssets::discover);
                let language = crate::ocr_model_catalog::model(index)
                    .map(|model| super::ocr_language_name(model.code))
                    .unwrap_or("OCR language");
                self.notice = Some(format!("Installed {language} OCR language pack"));
            }
            Err(error) => self.notice = Some(error),
        }
        Some(Change::Full)
    }
}

fn model_size(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
}
