//! Reader-wide preview and location history settings.
use super::{Action, Item, Reader, Screen};
use crate::reading_controls::SkimPosition;

impl Reader {
    pub(super) fn navigation_preference_items(&self) -> Vec<Item> {
        let controls = self.controls;
        let language = controls.interface_language;
        let mut opening = controls;
        opening.remember_opening_location = !opening.remember_opening_location;
        let mut items = vec![
            Item::new(
                language.navigation_settings_text("Add opening page to location history"),
                Action::SetReadingControls(opening),
            )
            .detail(
                language.navigation_settings_text("Uses your saved place when reopening a book"),
            ),
        ];
        items.push(Item::new(
            language.navigation_settings_text("Skim dialog position"),
            Action::Heading,
        ));
        items.extend(SkimPosition::ALL.map(|placement| {
            let mut next = controls;
            next.skim_position = placement;
            let item = Item::new(
                language.navigation_settings_text(placement.label()),
                Action::SetReadingControls(next),
            );
            if placement == controls.skim_position {
                item.detail(language.navigation_settings_text("Selected"))
            } else {
                item
            }
        }));
        items.extend(self.back_preference_entries());
        items.push(
            Item::new(
                language.navigation_settings_text("Assign a Back gesture"),
                Action::Show(Screen::Gestures),
            )
            .detail(
                language
                    .navigation_settings_text("Choose a gesture, then Back (navigation settings)"),
            ),
        );
        items
    }

    pub(super) fn remember_opening_location(&mut self) {
        if self.controls.remember_opening_location
            && let Some(location) = self.current_history_location()
        {
            self.push_history(location);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::{
        Action, Reader, Screen,
        tests::{Fixture, capture_test_screen},
    };
    use primitive_app::{App, Change};
    use primitive_ui::Input;

    fn tap_option(app: &mut Reader, index: usize) {
        let row = app.list_geometry().row(index);
        assert_eq!(
            app.input(Input::Tap((row.x + 20) as f32, (row.y + row.h / 2) as f32)),
            Change::Partial
        );
    }

    #[test]
    fn opening_location_records_exact_restored_epub_and_can_be_disabled() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Long chapter.epub");
        std::fs::copy(crate::test_fixture("long-chapter.epub"), &path).unwrap();
        let mut app = fixture.app();
        app.open_path(&path).unwrap();
        assert!(app.back.is_empty());
        app.active
            .as_mut()
            .unwrap()
            .reading
            .go_to_page(0, 3)
            .unwrap();
        app.record_position();
        app.save().unwrap();
        let opening = app.current_history_location().unwrap();

        app.show(Screen::NavigationSettings);
        tap_option(&mut app, 0);
        assert!(app.controls.remember_opening_location && app.back.is_empty());
        let fonts = [
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        capture_test_screen(&app, &fonts, "READER_NAVIGATION_PREFERENCES", false);
        let mut app = fixture.app();
        app.open_path(&path).unwrap();
        assert_eq!(app.back, vec![opening.clone()]);
        app.active
            .as_mut()
            .unwrap()
            .reading
            .go_to_page(0, 4)
            .unwrap();
        assert_ne!(app.current_history_location().as_ref(), Some(&opening));
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_history_location().as_ref(), Some(&opening));
        app.show(Screen::NavigationSettings);
        tap_option(&mut app, 0);
        app.open_path(&path).unwrap();
        assert!(!app.controls.remember_opening_location && app.back.is_empty());
    }

    #[test]
    fn opening_location_records_restored_pdf_page_and_clears_when_leaving() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Tilted scan.pdf");
        std::fs::copy(crate::test_fixture("tilted-scan.pdf"), &path).unwrap();
        let mut app = fixture.app();
        app.open_path(&path).unwrap();
        app.follow_fixed(1, None).unwrap();
        app.save().unwrap();
        app.show(Screen::NavigationSettings);
        tap_option(&mut app, 0);
        let mut app = fixture.app();
        app.open_path(&path).unwrap();
        let opening = app.current_history_location().unwrap();
        assert_eq!(opening.position.page, 1);
        assert_eq!(app.back, vec![opening.clone()]);
        app.fixed.as_mut().unwrap().reading.jump(0).unwrap();
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_history_location().as_ref(), Some(&opening));
        app.close_book_to_library();
        assert!(app.back.is_empty() && app.forward.is_empty());
    }
}
