use super::*;

impl Reader {
    pub(super) fn dictionary_catalog_rows(&self) -> Vec<Item> {
        let mut rows = vec![if self.dictionary_catalog_refresh.is_some() {
            Item::new("Refreshing dictionary catalogue…", Action::Heading)
        } else {
            Item::new(
                "Refresh dictionary catalogue",
                Action::RefreshDictionaryCatalog,
            )
            .detail(format!(
                "{} packages · KOReader revision {}",
                self.dictionary_catalog.packages.len(),
                self.dictionary_catalog.revision
            ))
        }];
        rows.extend(
            crate::dictionary_catalog::language_groups(&self.dictionary_catalog)
                .iter()
                .enumerate()
                .map(|(index, group)| {
                    let count = group.packages.len();
                    let action = if self.dictionary_catalog_refresh.is_some() {
                        Action::Heading
                    } else {
                        Action::Show(Screen::DictionaryCatalogLanguage(index))
                    };
                    Item::new(group.language.clone(), action).detail(format!(
                        "{count} {}",
                        self.controls
                            .interface_language
                            .screen_title(if count == 1 {
                                "dictionary"
                            } else {
                                "dictionaries"
                            })
                    ))
                }),
        );
        rows
    }

    pub(super) fn dictionary_catalog_language_rows(&self, group_index: usize) -> Vec<Item> {
        let groups = crate::dictionary_catalog::language_groups(&self.dictionary_catalog);
        let language = self.controls.interface_language;
        let Some(group) = groups.get(group_index) else {
            return vec![Item::new(
                language.screen_title("Language unavailable"),
                Action::Heading,
            )];
        };
        let mut rows = vec![
            Item::new(group.language.clone(), Action::Heading),
            Item::new(
                language.screen_title("Filter dictionaries"),
                Action::CycleDictionaryCatalogFilter,
            )
            .detail(language.screen_title(self.dictionary_catalog_filter.label())),
        ];
        rows.extend(
            group
                .packages
                .iter()
                .copied()
                .filter_map(|index| {
                    let package = self.dictionary_catalog.packages.get(index)?;
                    self.dictionary_catalog_filter
                        .accepts(package)
                        .then_some((index, package))
                })
                .map(|(index, package)| {
                    let installed = crate::dictionary_catalog::installed(
                        &self.dictionaries.local_directory(),
                        package,
                    );
                    let detail = format!(
                        "{} {} · {}{}",
                        package.entries,
                        language.screen_title("entries"),
                        package.language_pair(),
                        if installed {
                            format!(" · {}", language.screen_title("Installed"))
                        } else {
                            String::new()
                        }
                    );
                    Item::new(
                        package.title.clone(),
                        Action::Show(Screen::DictionaryCatalogItem(group_index, index)),
                    )
                    .detail(detail)
                }),
        );
        if rows.len() == 2 {
            rows.push(Item::new(
                language.screen_title("No dictionaries match this filter"),
                Action::Heading,
            ));
        }
        rows
    }

    pub(super) fn dictionary_catalog_item_rows(
        &self,
        group_index: usize,
        index: usize,
    ) -> Vec<Item> {
        let language = self.controls.interface_language;
        let Some(package) = self.dictionary_catalog.packages.get(index) else {
            return vec![Item::new(
                language.screen_title("Dictionary unavailable"),
                Action::Heading,
            )];
        };
        let mut rows = vec![
            Item::new(language.screen_title("Language pair"), Action::Heading)
                .detail(package.language_pair()),
            Item::new(language.screen_title("Entries"), Action::Heading)
                .detail(package.entries.to_string()),
            Item::new(language.screen_title("License"), Action::Heading).detail(
                if package.license.is_empty() {
                    "Not stated in the KOReader catalogue".to_owned()
                } else {
                    package.license.clone()
                },
            ),
            Item::new(
                language.screen_title("License information"),
                Action::Heading,
            )
            .detail("This is source catalogue text, not a legal review of the dictionary data."),
            Item::new(language.screen_title("Package source"), Action::Heading)
                .detail(package.url.clone()),
            Item::new(language.screen_title("Catalogue source"), Action::Heading)
                .detail("KOReader dictionary catalogue, pinned revision 9192014d"),
        ];
        if crate::dictionary_catalog::installed(&self.dictionaries.local_directory(), package) {
            rows.push(
                Item::new(language.screen_title("Installed"), Action::Heading)
                    .detail(language.screen_title("Available in dictionary lookup")),
            );
        } else if self
            .dictionary_size_check
            .as_ref()
            .is_some_and(|job| job.package_slug() == package.slug.as_str())
        {
            rows.push(
                Item::new("Checking package size…", Action::Heading)
                    .detail("Reader is checking the download size over HTTPS"),
            );
        } else if self.dictionary_size_check.is_some() {
            rows.push(Item::new(
                "Another dictionary package check is in progress",
                Action::Heading,
            ));
        } else if let Some(job) = self.dictionary_download.as_ref() {
            if job.package() == package.slug.as_str() {
                if job.cancel_requested() {
                    rows.push(Item::new("Canceling dictionary download…", Action::Heading));
                } else {
                    rows.push(
                        Item::new("Download progress", Action::Heading)
                            .detail(job.progress_label().unwrap_or_else(|| "Connecting…".into())),
                    );
                    rows.push(
                        Item::new("Cancel download", Action::CancelDictionaryDownload)
                            .detail("Temporary files are removed when canceled"),
                    );
                }
            } else {
                rows.push(Item::new(
                    "Another dictionary download is in progress",
                    Action::Heading,
                ));
            }
        } else if !package.installable() {
            rows.push(
                Item::new("HTTPS download unavailable", Action::Heading)
                    .detail("Reader only downloads supported archives over HTTPS"),
            );
        } else {
            let estimate = self.dictionary_size_estimate.as_ref().filter(
                |(slug, _, checked_at)| {
                    slug.as_str() == package.slug.as_str()
                        && checked_at.elapsed() <= Duration::from_secs(600)
                },
            );
            match estimate.map(|(_, result, _)| result) {
                None => rows.push(
                    Item::new("Check package size", Action::CheckDictionarySize(group_index, index))
                        .detail("Check the archive size before downloading"),
                ),
                Some(Ok(Some(size))) if *size > crate::dictionary_catalog::MAX_COMPRESSED_BYTES as u64 => {
                    rows.push(
                        Item::new("Package exceeds Reader's download limit", Action::Heading)
                            .detail(format!(
                                "Archive size: {} · limit: {}",
                                crate::dictionary_catalog::format_download_size(*size),
                                crate::dictionary_catalog::format_download_size(
                                    crate::dictionary_catalog::MAX_COMPRESSED_BYTES as u64
                                )
                            )),
                    );
                }
                Some(Ok(Some(size))) => rows.push(
                    Item::new(
                        language.screen_title("Download and install"),
                        Action::InstallDictionary(group_index, index),
                    )
                    .detail(format!(
                        "Archive size: {} · downloads over HTTPS",
                        crate::dictionary_catalog::format_download_size(*size)
                    )),
                ),
                Some(Ok(None)) => {
                    rows.push(
                        Item::new("Archive size unavailable", Action::Heading)
                            .detail("The server did not provide a Content-Length"),
                    );
                    rows.push(
                        Item::new(
                            "Download without size estimate",
                            Action::DownloadDictionaryWithoutSizeEstimate(group_index, index),
                        )
                        .detail("Reader still enforces its compressed archive limit"),
                    );
                    rows.push(Item::new(
                        "Retry size check",
                        Action::CheckDictionarySize(group_index, index),
                    ));
                }
                Some(Err(error)) => {
                    rows.push(
                        Item::new("Package size check failed", Action::Heading)
                            .detail(error.clone()),
                    );
                    rows.push(
                        Item::new(
                            "Download without size estimate",
                            Action::DownloadDictionaryWithoutSizeEstimate(group_index, index),
                        )
                        .detail("Reader still enforces its compressed archive limit"),
                    );
                    rows.push(Item::new(
                        "Retry size check",
                        Action::CheckDictionarySize(group_index, index),
                    ));
                }
            }
        }
        rows
    }

    pub(super) fn install_dictionary(&mut self, group_index: usize, index: usize) -> Result<()> {
        let package = self
            .dictionary_catalog
            .packages
            .get(index)
            .cloned()
            .ok_or("That dictionary is not in the catalogue")?;
        if self.dictionary_download.is_some()
            || self.ocr_model_download.is_some()
            || self.dictionary_size_check.is_some()
        {
            return Err("Another Reader package download is already in progress".into());
        }
        if !package.installable() {
            return Err(
                "Only HTTPS dictionary archives in supported formats can be installed".into(),
            );
        }
        let estimate = self.dictionary_size_estimate.as_ref().filter(
            |(slug, _, checked_at)| {
                slug.as_str() == package.slug.as_str()
                    && checked_at.elapsed() <= Duration::from_secs(600)
            },
        );
        match estimate.map(|(_, result, _)| result) {
            Some(Ok(Some(size)))
                if *size <= crate::dictionary_catalog::MAX_COMPRESSED_BYTES as u64 =>
            {
                self.start_dictionary_download(package, group_index, index)
            }
            Some(Ok(Some(_))) => {
                Err("This dictionary archive exceeds Reader's download limit".into())
            }
            _ => Err("Check the package size or choose Download without size estimate".into()),
        }
    }

    pub(super) fn download_dictionary_without_size_estimate(
        &mut self,
        group_index: usize,
        index: usize,
    ) -> Result<()> {
        let package = self
            .dictionary_catalog
            .packages
            .get(index)
            .cloned()
            .ok_or("That dictionary is not in the catalogue")?;
        let estimate = self.dictionary_size_estimate.as_ref().filter(
            |(slug, _, checked_at)| {
                slug.as_str() == package.slug.as_str()
                    && checked_at.elapsed() <= Duration::from_secs(600)
            },
        );
        match estimate.map(|(_, result, _)| result) {
            Some(Ok(None) | Err(_)) => {
                self.start_dictionary_download(package, group_index, index)
            }
            Some(Ok(Some(_))) => {
                Err("This package has a size estimate; use Download and install".into())
            }
            None => Err("Check the package size before downloading".into()),
        }
    }

    fn start_dictionary_download(
        &mut self,
        package: crate::dictionary_catalog::Package,
        group_index: usize,
        index: usize,
    ) -> Result<()> {
        if self.dictionary_download.is_some()
            || self.ocr_model_download.is_some()
            || self.dictionary_size_check.is_some()
        {
            return Err("Another Reader package download is already in progress".into());
        }
        if !package.installable() {
            return Err(
                "Only HTTPS dictionary archives in supported formats can be installed".into(),
            );
        }
        if crate::dictionary_catalog::installed(&self.dictionaries.local_directory(), &package) {
            self.notice = Some("This dictionary is already installed".into());
            return Ok(());
        }
        let job = crate::dictionary_catalog::Job::start(
            package.clone(),
            self.dictionaries.local_directory(),
        )?;
        self.show(Screen::DictionaryCatalogItem(group_index, index));
        self.dictionary_download = Some(job);
        self.notice = Some(format!("Downloading {} over HTTPS…", package.title));
        Ok(())
    }

    pub(super) fn check_dictionary_size(&mut self, group_index: usize, index: usize) -> Result<()> {
        let package = self
            .dictionary_catalog
            .packages
            .get(index)
            .cloned()
            .ok_or("That dictionary is not in the catalogue")?;
        if self.dictionary_download.is_some()
            || self.ocr_model_download.is_some()
            || self.dictionary_size_check.is_some()
        {
            return Err("Another Reader package operation is already in progress".into());
        }
        if !package.installable() {
            return Err("Only HTTPS dictionary archives in supported formats can be checked".into());
        }
        if crate::dictionary_catalog::installed(&self.dictionaries.local_directory(), &package) {
            self.notice = Some("This dictionary is already installed".into());
            return Ok(());
        }
        let job = crate::dictionary_catalog::SizeCheckJob::start(&package)?;
        self.dictionary_size_estimate = None;
        self.dictionary_size_check = Some(job);
        self.show(Screen::DictionaryCatalogItem(group_index, index));
        self.notice = Some(format!("Checking {} package size over HTTPS…", package.title));
        Ok(())
    }

    pub(super) fn poll_dictionary_size_check(&mut self, now: Instant) -> Option<Change> {
        let (slug, result) = {
            let job = self.dictionary_size_check.as_mut()?;
            match job.poll(now) {
                crate::dictionary_catalog::SizeCheckPoll::Pending => return None,
                crate::dictionary_catalog::SizeCheckPoll::Finished(result) => {
                    (job.package_slug().to_owned(), result)
                }
            }
        };
        self.dictionary_size_check = None;
        let notice = match &result {
            Ok(Some(size)) => format!(
                "Dictionary archive size: {}",
                crate::dictionary_catalog::format_download_size(*size)
            ),
            Ok(None) => "The dictionary server did not provide a package size".into(),
            Err(error) => format!("Could not check dictionary package size: {error}"),
        };
        self.dictionary_size_estimate = Some((slug, result, now));
        self.notice = Some(notice);
        Some(Change::Full)
    }

    pub(super) fn open_dictionary_catalog(&mut self) {
        self.show(Screen::DictionaryCatalog);
        if !self.dictionary_catalog.cached
            && self.dictionary_catalog_refresh.is_none()
            && !self.dictionary_catalog_refresh_attempted
        {
            self.refresh_dictionary_catalog();
        }
    }

    pub(super) fn refresh_dictionary_catalog(&mut self) {
        if self.dictionary_catalog_refresh.is_some() {
            return;
        }
        self.dictionary_catalog_refresh_attempted = true;
        match crate::dictionary_catalog::CatalogRefreshJob::start(
            self.dictionaries.local_directory(),
        ) {
            Ok(job) => {
                self.dictionary_catalog_refresh = Some(job);
                self.notice = Some("Refreshing dictionary catalogue over HTTPS…".into());
            }
            Err(error) => self.notice = Some(error.to_string()),
        }
    }

    pub(super) fn poll_dictionary_catalog_refresh(&mut self, now: Instant) -> Option<Change> {
        let job = self.dictionary_catalog_refresh.as_mut()?;
        match job.poll(now) {
            crate::dictionary_catalog::CatalogPoll::Pending => None,
            crate::dictionary_catalog::CatalogPoll::Finished(Ok(catalog)) => {
                let count = catalog.packages.len();
                self.dictionary_catalog = catalog;
                self.dictionary_catalog_refresh = None;
                self.notice = Some(format!("Loaded {count} dictionaries from KOReader"));
                Some(Change::Full)
            }
            crate::dictionary_catalog::CatalogPoll::Finished(Err(error)) => {
                self.dictionary_catalog_refresh = None;
                self.notice = Some(format!("Dictionary catalogue refresh failed: {error}"));
                Some(Change::Full)
            }
        }
    }

    pub(super) fn cycle_dictionary_catalog_filter(&mut self) {
        self.dictionary_catalog_filter = self.dictionary_catalog_filter.next();
    }

    pub(super) fn cancel_dictionary_download(&mut self) {
        let Some(job) = self.dictionary_download.as_mut() else {
            return;
        };
        if !job.cancel_requested() {
            job.cancel();
            self.notice = Some("Canceling dictionary download…".into());
        }
    }

    pub(super) fn poll_dictionary_download(&mut self, now: Instant) -> Option<Change> {
        let poll = self.dictionary_download.as_mut()?.poll(now);
        let result = match poll {
            crate::dictionary_catalog::Poll::Pending { progress_changed } => {
                return progress_changed.then_some(Change::Partial);
            }
            crate::dictionary_catalog::Poll::Finished(result) => result,
        };
        self.dictionary_download = None;
        match result {
            Ok(title) => match self.refresh_installed_dictionaries() {
                Ok(()) => {
                    self.sort_dictionaries();
                    self.notice = Some(format!("Installed {title}"));
                }
                Err(error) => {
                    self.notice = Some(format!(
                        "Installed {title}; dictionary list needs refresh: {error}"
                    ));
                }
            },
            Err(error) => self.notice = Some(error),
        }
        Some(Change::Full)
    }
}
