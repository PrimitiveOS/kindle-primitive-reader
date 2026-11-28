use super::*;
use crate::app::tests::Fixture;

#[test]
fn character_pages_count_a_plain_text_book_too() {
    let fixture = Fixture::new();
    let book = fixture.root.join("Harbor notes.txt");
    let paragraph = "The tide turns at dusk and the channel fills with slow water. ";
    let text: String = (0..60).map(|_| format!("{}\n\n", paragraph.repeat(4))).collect();
    std::fs::write(&book, text).unwrap();
    let mut app = fixture.app();
    app.open_path(&book).unwrap();
    app.set_character_pages(Some(500)).unwrap();
    let stop = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while app.active.as_ref().unwrap().numbering.is_none() {
        assert!(std::time::Instant::now() < stop, "character pages did not finish");
        app.step_character_pages(std::time::Instant::now()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(app.active.as_ref().unwrap().numbering.as_ref().unwrap().stride, 500);
}
