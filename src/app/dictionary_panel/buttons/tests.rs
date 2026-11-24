use super::*;
use crate::app::selection_popup::SelectionAction;
use crate::app::selection_viewport_tests::{adopt, capture, tap};
use crate::app::tests::tap_contents_item as choose;
use crate::session::ReadingViewport;

fn back(app: &mut Reader) {
    let button = app.navigation_layout().back();
    tap(app, button);
}

fn rectangle(grid: &Grid, target: Button) -> Rect {
    grid.items
        .iter()
        .find(|(button, _)| *button == target)
        .unwrap()
        .1
}

#[test]
fn dictionary_button_editor_previews_stages_and_persists_order_visibility_and_limits() {
    let mut simulator = super::super::tests::Simulation::new();
    let app = &mut simulator.app;
    app.activate(Action::ShowDictionaries).unwrap();
    choose(app, "Customize buttons");
    choose(app, "Sort and toggle buttons");
    choose(app, "Next result");
    choose(app, "Move earlier");
    back(app);
    choose(app, "Translate");
    choose(app, "Show button");
    back(app);
    back(app);
    choose(app, "Max buttons in row 1");
    choose(app, "4");
    choose(app, "Test button layout");
    assert!(app.dictionary_settings.popup_buttons.is_none());
    let mut landscape = adopt(app, ReadingViewport::LANDSCAPE, 1);
    capture(
        app,
        &mut landscape,
        "landscape-dictionary-button-layout-preview.png",
        true,
    );
    back(app);
    back(app);
    assert!(app.dictionary_settings.popup_buttons.is_none());

    choose(app, "Customize buttons");
    choose(app, "Sort and toggle buttons");
    choose(app, "Next result");
    choose(app, "Move earlier");
    back(app);
    choose(app, "Translate");
    choose(app, "Show button");
    back(app);
    choose(app, "Wikipedia");
    choose(app, "Show button");
    back(app);
    back(app);
    choose(app, "Max buttons in row 1");
    choose(app, "4");
    capture(
        app,
        &mut landscape,
        "landscape-dictionary-customize-buttons.png",
        false,
    );
    choose(app, "Apply layout");
    let prefs = app.dictionary_settings.popup_buttons.clone().unwrap();
    assert_eq!(prefs.capacity(0), 4);
    assert_eq!(prefs.order[6], Button::Next);
    assert!(!prefs.visible.contains(&Button::Wikipedia));
    assert!(prefs.visible.contains(&Button::Translate));

    let saved = fs::read(&app.dictionary_preference).unwrap();
    let mut backup = SourcePreferences::backup_value(&saved).unwrap();
    assert_eq!(backup["popup_buttons"]["limits"][0], 4);
    backup["popup_buttons"]["limits"][0] = serde_json::json!(0);
    assert!(SourcePreferences::backup_value(backup.to_string().as_bytes()).is_err());

    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let fixture = &simulator.fixture;
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    assert_eq!(reopened.dictionary_settings.popup_buttons, Some(prefs));
    reopened.activate(Action::ShowDictionaries).unwrap();
    choose(&mut reopened, "Customize buttons");
    choose(&mut reopened, "Restore default layout");
    choose(&mut reopened, "Restore default layout");
    assert!(reopened.dictionary_settings.popup_buttons.is_none());
}

#[test]
fn dictionary_button_geometry_keeps_the_four_button_pair_and_paged_rows_separate() {
    let mut config = Settings {
        order: vec![
            Button::Previous,
            Button::Highlight,
            Button::Translate,
            Button::Next,
            Button::SaveWord,
            Button::Find,
            Button::Wikipedia,
            Button::SearchBook,
            Button::Expand,
            Button::Close,
        ],
        visible: Button::ALL.to_vec(),
        limits: vec![4, 1, 1, 1, 1, 1, 1],
    };
    assert!(config.is_valid());
    let popup = Rect::new(0, 0, 1136, 1018);
    let grid = Grid::configured(popup, &config, 0);
    let previous = rectangle(&grid, Button::Previous);
    let next = rectangle(&grid, Button::Next);
    assert_eq!(previous.w, next.w);
    assert_eq!(previous.y, next.y);
    assert!(previous.w < rectangle(&grid, Button::Translate).w);
    let mut shown = Vec::new();
    for page in 0..3 {
        let grid = Grid::configured(popup, &config, page);
        for &(button, rect) in &grid.items {
            assert!(rect.y >= popup.bottom() - config.footer_height());
            assert!(rect.bottom() <= popup.bottom() - ROW_HEIGHT - GAP);
            assert!(!shown.contains(&button));
            shown.push(button);
        }
    }
    assert_eq!(shown, config.order);
    config.visible.clear();
    assert!(config.is_valid());
    assert!(Grid::configured(popup, &config, 0).items.is_empty());
}

#[test]
fn dictionary_button_layout_runs_actions_and_preserves_the_selected_passage() {
    let mut simulator = super::super::tests::Simulation::new();
    simulator.open_epub_selection();
    let selection = simulator.app.selection.clone();
    let location = simulator.app.current_history_location();
    let config = Settings {
        order: vec![
            Button::Next,
            Button::Highlight,
            Button::Translate,
            Button::Previous,
            Button::Close,
            Button::SaveWord,
            Button::Find,
            Button::Wikipedia,
            Button::SearchBook,
            Button::Expand,
        ],
        visible: vec![
            Button::Next,
            Button::Highlight,
            Button::Translate,
            Button::Previous,
            Button::Close,
        ],
        limits: vec![1; 5],
    };
    simulator.app.dictionary_settings.popup_buttons = Some(config);
    simulator.tap(simulator.app.selection_button(SelectionAction::Dictionary));
    simulator.draw(Some("dictionary-popup-custom-buttons.png"));
    let grid = simulator
        .app
        .definition_button_grid(simulator.app.definition_layout().0);
    let next = rectangle(&grid, Button::Next);
    assert_eq!(
        simulator
            .app
            .input(Input::LongTap((next.x + 16) as f32, (next.y + 16) as f32)),
        Change::Partial
    );
    let index = simulator.app.definitions.len() - 1;
    assert_eq!(simulator.app.list_page, index);
    simulator.tap(rectangle(&grid, Button::Translate));
    assert_eq!(simulator.app.screen, Screen::OnlineLookup);
    let prompt = simulator.app.online.as_ref().unwrap();
    assert_eq!(prompt.request.text, "the");
    assert!(prompt.job.is_none());
    simulator.tap(crate::app::online_panel::CLOSE);
    assert_eq!(simulator.app.screen, Screen::Definition);
    assert_eq!(simulator.app.list_page, index);
    let grid = simulator
        .app
        .definition_button_grid(simulator.app.definition_layout().0);
    simulator.tap(rectangle(&grid, Button::Highlight));
    let page_forward = grid.pager[0].1;
    simulator.tap(page_forward);
    let grid = simulator
        .app
        .definition_button_grid(simulator.app.definition_layout().0);
    assert!(
        grid.items
            .iter()
            .all(|(button, _)| !matches!(button, Button::Translate))
    );
    simulator.tap(rectangle(&grid, Button::Close));
    assert_eq!(simulator.app.screen, Screen::Book);
    assert_eq!(simulator.app.current_history_location(), location);
    let selected = selection.unwrap();
    assert!(
        simulator
            .app
            .highlight_state()
            .unwrap()
            .highlights
            .iter()
            .any(|mark| mark.selection == selected)
    );

    simulator
        .app
        .dictionary_settings
        .popup_buttons
        .as_mut()
        .unwrap()
        .visible
        .clear();
    simulator.app.look_up("beacon").unwrap();
    assert!(
        simulator
            .app
            .definition_button_grid(simulator.app.definition_layout().0)
            .items
            .is_empty()
    );
    simulator.tap(simulator.app.definition_layout().1);
    assert_eq!(simulator.app.screen, Screen::Book);
}
