use super::font_defaults::Role;
use super::*;
use crate::app::tests::Fixture;
use primitive_app::App;
use std::fs;

fn fonts_dir(fixture: &Fixture) -> PathBuf {
    let dir = fixture.root.join("fonts");
    fs::create_dir_all(&dir).unwrap();
    fs::copy(
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        dir.join("NotoSans-Regular.ttf"),
    )
    .unwrap();
    fs::copy(
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        dir.join("NotoSans-Bold.ttf"),
    )
    .unwrap();
    dir
}

fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap()
}

#[test]
fn the_default_font_leads_books_without_their_own_font_and_survives_reopening() {
    let fixture = Fixture::new();
    let dir = fonts_dir(&fixture);
    let mut app = fixture.app();
    app.open_path(&fixture.book).unwrap();
    let names = crate::reading_fonts::available(&dir).unwrap();
    let first = names[0].clone();

    app.activate(Action::CycleFontRole(Role::Default)).unwrap();
    assert_eq!(app.font_defaults.get(Role::Default), Some(first.as_str()));
    assert_eq!(canonical(&app.fonts_for(None).unwrap()[0]), canonical(&dir.join(&first)));

    app.save().unwrap();
    let fonts = vec![crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(reopened.font_defaults.get(Role::Default), Some(first.as_str()));
    assert_eq!(canonical(&reopened.fonts_for(None).unwrap()[0]), canonical(&dir.join(&first)));
}

#[test]
fn a_monospace_choice_becomes_an_engine_role_and_cycles_back_to_off() {
    let fixture = Fixture::new();
    let dir = fonts_dir(&fixture);
    let mut app = fixture.app();
    app.open_path(&fixture.book).unwrap();
    let names = crate::reading_fonts::available(&dir).unwrap();
    assert!(app.font_roles().is_empty());

    app.activate(Action::CycleFontRole(Role::Monospace)).unwrap();
    let chosen = app.font_defaults.get(Role::Monospace).unwrap().to_owned();
    let roles = app.font_roles();
    let [(generic, path)] = roles.entries() else {
        panic!("one monospace role, not {:?}", roles.entries());
    };
    assert_eq!(*generic, parley::GenericFamily::Monospace);
    assert_eq!(canonical(path), canonical(&dir.join(&chosen)));

    // One step per font, then Off again.
    for _ in 0..names.len() {
        app.activate(Action::CycleFontRole(Role::Monospace)).unwrap();
    }
    assert_eq!(app.font_defaults.get(Role::Monospace), None);
    assert!(app.font_roles().is_empty());
}
