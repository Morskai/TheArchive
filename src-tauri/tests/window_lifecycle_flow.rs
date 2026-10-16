use local_film_library_preview_lib::window_lifecycle::{CloseAction, close_action};
#[test]
fn ordinary_close_and_tray_exit_only_use_the_close_preference() {
    assert_eq!(close_action(true, false, true), CloseAction::Exit);
    assert_eq!(close_action(true, false, false), CloseAction::Exit);
    assert_eq!(close_action(false, false, true), CloseAction::Hide);
    assert_eq!(close_action(false, true, true), CloseAction::Exit);
    assert_eq!(close_action(false, true, false), CloseAction::Exit);
}

#[test]
fn unavailable_tray_offers_an_ordinary_exit_instead_of_hiding_the_window() {
    assert_eq!(close_action(false, false, false), CloseAction::Confirm);
}
