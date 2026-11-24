//! Apply reading profiles while the book being closed still owns its source.
use super::opening_rules::{Event, Page};
use super::{
    Action, Canvas, Change, FileBrowser, Input, Item, LibraryBook, Profile, Reader, Rect, Result,
    Screen, Sheet, SheetAction,
};
use crate::app::bookmark_browser::MarkKey;
use crate::reading_profiles::{OnClose, opening::Book};
use primitive_app::App;
use std::path::PathBuf;
use std::time::Instant;

pub(in crate::app) enum AfterClose {
    Library,
    Exit,
    Open {
        entry: LibraryBook,
        password_return: Option<Screen>,
    },
    Mark(MarkKey),
    CurrentFolder {
        browser: FileBrowser,
        path: PathBuf,
    },
    FinishedFolder(PathBuf),
}

struct Choice {
    profile: Profile,
    accepted: bool,
    detail: Option<String>,
}

pub(in crate::app) struct Closing {
    book: u64,
    epoch: u64,
    next: AfterClose,
    choices: Vec<Choice>,
}

pub(in crate::app) struct BeforeSwitch {
    book: u64,
    epoch: u64,
    profiles: Vec<Profile>,
}

impl Closing {
    fn asking(&self) -> Option<&str> {
        self.choices
            .iter()
            .find_map(|choice| choice.detail.as_deref())
    }

    fn decide(&mut self, accepted: bool) {
        if let Some(choice) = self
            .choices
            .iter_mut()
            .find(|choice| choice.detail.is_some())
        {
            choice.detail = None;
            choice.accepted = accepted;
        }
    }
}

fn policy_label(policy: Option<OnClose>, limited: bool) -> &'static str {
    match policy {
        None => "Off",
        Some(OnClose::Apply) if limited => "Apply when matched",
        Some(OnClose::Apply) => "Always apply",
        Some(OnClose::Ask) => "Ask to execute",
    }
}

pub(super) fn summary(profile: &Profile) -> &'static str {
    policy_label(
        profile.apply_on_close(),
        profile.closing_options().is_some(),
    )
}

impl Reader {
    pub(super) fn close_profile_items(&self, index: usize) -> Vec<Item> {
        let Some(profile) = self.profiles.get(index) else {
            return Vec::new();
        };
        let options = profile.closing_options();
        let mut items: Vec<_> = [None, Some(OnClose::Apply), Some(OnClose::Ask)]
            .into_iter()
            .map(|policy| {
                Item::new(
                    policy_label(policy, options.is_some()),
                    Action::SetClosingProfile(index, policy),
                )
                .detail(if policy == profile.apply_on_close() {
                    "Selected"
                } else {
                    ""
                })
            })
            .collect();
        items.extend([
            Item::new(
                "Closing conditions",
                Action::Show(Event::Closing.screen(index, Page::Conditions)),
            )
            .detail(
                if options.is_some_and(|options| options.conditions.has_matchers()) {
                    "Match any selected condition"
                } else {
                    "Any book"
                },
            ),
            Item::new(
                "Only within time window",
                Action::Show(Event::Closing.screen(index, Page::TimeWindow)),
            )
            .detail(
                options
                    .and_then(|options| options.time)
                    .map_or_else(|| "Any time".into(), super::opening_time::window_label),
            ),
        ]);
        items
    }

    pub(in crate::app) fn set_closing_profile(
        &mut self,
        index: usize,
        policy: Option<OnClose>,
    ) -> Result<()> {
        let mut profiles = self.profiles.clone();
        profiles
            .get_mut(index)
            .ok_or("That profile is unavailable")?
            .set_apply_on_close(policy);
        self.commit_profiles(profiles)?;
        self.notice = None;
        Ok(())
    }

    pub(in crate::app) fn request_close(&mut self, next: AfterClose) -> Result<Change> {
        if self.closing_profile.is_some() {
            return Ok(Change::None);
        }
        self.closing_before_switch = None;
        if let AfterClose::Open { entry, .. } = &next
            && self
                .current_document()
                .is_some_and(|(current, _)| current.path == entry.path)
        {
            return self.finish_close_route(next, false);
        }
        let Some(book) = self.open_book_id() else {
            return self.finish_close_route(next, false);
        };
        let mut choices: Vec<_> = self
            .matching_closing_profiles(self.automatic_profile_minute(Event::Closing))
            .into_iter()
            .map(|(profile, policy)| {
                let detail = (policy == OnClose::Ask)
                    .then(|| format!("{}\nCancel skips this profile.", profile.name()));
                Choice {
                    profile: profile.clone(),
                    accepted: policy == OnClose::Apply,
                    detail,
                }
            })
            .collect();
        if choices.is_empty() {
            return self.finish_close_route(next, false);
        }
        choices.sort_by_cached_key(|choice| choice.profile.name().to_lowercase());
        self.opening_execution = None;
        self.settle_pending_opening();
        let request = Closing {
            book,
            epoch: self.opening_profile_epoch,
            next,
            choices,
        };
        if request.asking().is_some() {
            if let Some(book) = &mut self.active {
                book.reading.pause_turns();
            }
            if let Some(book) = &mut self.fixed {
                book.reading.pause();
            }
            self.closing_profile = Some(request);
            self.sync_auto_turn(Instant::now());
            self.update_stats_activity(Instant::now());
            Ok(Change::Partial)
        } else {
            self.complete_closing_profiles(request)
        }
    }

    fn matching_closing_profiles(&self, minute: Option<u16>) -> Vec<(&Profile, OnClose)> {
        let Some((entry, id)) = self.current_document() else {
            return Vec::new();
        };
        let eligible = |profile: &&Profile| {
            profile.apply_on_close().is_some() && self.profile_supported(profile)
        };
        let metadata = self
            .profiles
            .iter()
            .filter(eligible)
            .any(|profile| {
                profile
                    .closing_options()
                    .is_some_and(|options| !options.conditions.metadata.is_empty())
            })
            .then(|| self.current_opening_metadata())
            .flatten();
        let orientation = self
            .accepted_viewport
            .and_then(|accepted| {
                super::on_open::TURNS
                    .iter()
                    .find_map(|&(turn, orientation)| (accepted.turn == turn).then_some(orientation))
            })
            .unwrap_or_else(|| match self.reading_orientation() {
                super::ReadingOrientation::FollowLauncher => super::ReadingOrientation::Upright,
                orientation => orientation,
            });
        let context = Book {
            id,
            path: entry.library_path(),
            library: &self.library,
            orientation,
            is_new: false,
            metadata: metadata.as_ref(),
            local_minute: minute,
            download_folder: self.browser.directory(),
        };
        self.profiles
            .iter()
            .filter(eligible)
            .filter_map(|profile| Some((profile, profile.matches_closing(&context)?)))
            .collect()
    }

    fn complete_closing_profiles(&mut self, request: Closing) -> Result<Change> {
        if self.open_book_id() != Some(request.book) || self.opening_profile_epoch != request.epoch
        {
            return Err("The book changed before closing. Close it again.".into());
        }
        self.close_fixed_search_fit()?;
        self.save()?;
        let approved: Vec<_> = request
            .choices
            .into_iter()
            .filter(|choice| choice.accepted)
            .map(|choice| choice.profile)
            .collect();
        let warning = match request.next {
            AfterClose::Open { .. } | AfterClose::Mark(_) => {
                self.closing_before_switch = Some(BeforeSwitch {
                    book: request.book,
                    epoch: request.epoch,
                    profiles: approved,
                });
                None
            }
            _ => self.closing_profile_warning(&approved),
        };
        let next_result = self.finish_close_route(request.next, true);
        if self.screen != Screen::PdfPassword || self.locked.is_none() {
            self.closing_before_switch = None;
        }
        let change = next_result?;
        if let Some(warning) = warning {
            if change == Change::Exit {
                eprintln!("{warning}");
            } else {
                self.notice = Some(warning);
            }
        }
        Ok(change)
    }

    fn closing_profile_warning(&mut self, approved: &[Profile]) -> Option<String> {
        if approved.is_empty() {
            return None;
        }
        self.apply_profile_batch(approved)
            .err()
            .map(|error| format!("Closing profile could not be applied: {error}"))
    }

    // The next provider has fully prepared its initial reading session. Until
    // this point the outgoing book remains active, even over a password retry.
    pub(in crate::app) fn apply_closing_before_switch(&mut self, incoming: u64) -> Option<String> {
        let request = self.closing_before_switch.take()?;
        if incoming != request.book
            && self.open_book_id() == Some(request.book)
            && self.opening_profile_epoch == request.epoch
        {
            self.closing_profile_warning(&request.profiles)
        } else {
            None
        }
    }

    fn finish_close_route(
        &mut self,
        next: AfterClose,
        saved_before_profile: bool,
    ) -> Result<Change> {
        match next {
            AfterClose::Library => {
                if !saved_before_profile {
                    self.save()?;
                }
                self.close_book_to_library();
                Ok(Change::Partial)
            }
            AfterClose::Exit => {
                self.opening_execution = None;
                if !saved_before_profile {
                    self.close_fixed_search_fit()?;
                    self.save()?;
                }
                Ok(Change::Exit)
            }
            AfterClose::Open {
                entry,
                password_return,
            } => {
                let origin =
                    password_return.or_else(|| saved_before_profile.then(|| self.surface_screen()));
                self.open_book_now(entry)?;
                if self.screen == Screen::PdfPassword && self.open_book_id().is_some() {
                    self.folder_password_return = origin;
                }
                Ok(Change::Partial)
            }
            AfterClose::Mark(key) => {
                self.view_bookmark_in_book_now(key)?;
                Ok(Change::Partial)
            }
            AfterClose::CurrentFolder { browser, path } => {
                if !saved_before_profile {
                    self.save()?;
                }
                self.close_book_to_library();
                self.browser = browser;
                self.reveal_book_row(&path);
                Ok(Change::Partial)
            }
            AfterClose::FinishedFolder(path) => {
                if !saved_before_profile {
                    self.save()?;
                }
                self.close_book_to_library();
                self.browse_from_book(&path)?;
                Ok(Change::Partial)
            }
        }
    }

    fn closing_profile_sheet(&self) -> Option<Sheet<'_>> {
        let detail = self.closing_profile.as_ref()?.asking()?;
        let (width, height) = self.reading_viewport().dimensions();
        Some(Sheet::centered(
            Rect::new(0, 0, width as i32, height as i32),
            "Apply before closing?",
            detail,
            "Apply",
        ))
    }

    pub(in crate::app) fn draw_closing_profile(&self, canvas: &mut Canvas) {
        if let Some(sheet) = self.closing_profile_sheet() {
            sheet.draw(canvas, None);
        }
    }

    pub(in crate::app) fn closing_profile_press(&self, input: Input) -> Option<Option<(f32, f32)>> {
        let sheet = self.closing_profile_sheet()?;
        Some(match input {
            Input::Tap(x, y)
                if [SheetAction::Cancel, SheetAction::Confirm]
                    .into_iter()
                    .any(|action| sheet.button(action).contains(x, y)) =>
            {
                Some((x, y))
            }
            _ => None,
        })
    }

    pub(in crate::app) fn closing_profile_input(&mut self, input: Input) -> Option<Change> {
        if matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        let sheet = self.closing_profile_sheet()?;
        if !self.stats_awake {
            return Some(Change::None);
        }
        let action = match input {
            Input::PageBack => Some(SheetAction::Cancel),
            Input::Tap(x, y) => sheet.hit(x, y),
            _ => None,
        };
        let Some(action) = action else {
            return Some(Change::None);
        };
        let mut request = self.closing_profile.take()?;
        request.decide(action == SheetAction::Confirm);
        if request.asking().is_some() {
            self.closing_profile = Some(request);
            return Some(Change::Partial);
        }
        let change = match self.complete_closing_profiles(request) {
            Ok(change) => change,
            Err(error) => self.report(error),
        };
        self.sync_auto_turn(Instant::now());
        self.update_stats_activity(Instant::now());
        Some(change)
    }
}

#[cfg(test)]
mod tests;
