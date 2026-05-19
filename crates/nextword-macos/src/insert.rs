//! Synthesise keystrokes to insert text at the active caret.
//!
//! We post CGEvents marked with `keytap::SYNTHETIC_MARK_FIELD` so our own
//! event tap ignores them. The pasteboard alternative would be faster but
//! pollutes the user's clipboard, so we type the word character-by-character.

use core_graphics::event::{
    CGEvent, CGEventField, CGEventTapLocation,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

use crate::keytap::SYNTHETIC_MARK_FIELD;

/// Insert `text` at the caret. Returns Err if event source creation fails.
pub fn insert_text(text: &str) -> Result<(), &'static str> {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "could not create CGEventSource")?;

    for c in text.chars() {
        post_unicode(&source, c)?;
    }
    Ok(())
}

fn post_unicode(source: &CGEventSource, c: char) -> Result<(), &'static str> {
    let mut buf = [0u16; 2];
    let utf16 = c.encode_utf16(&mut buf);

    let key_down = CGEvent::new_keyboard_event(source.clone(), 0, true)
        .map_err(|_| "could not create keyDown event")?;
    key_down.set_string_from_utf16_unchecked(utf16);
    key_down.set_integer_value_field(SYNTHETIC_MARK_FIELD as CGEventField, 1);
    key_down.post(CGEventTapLocation::HID);

    let key_up = CGEvent::new_keyboard_event(source.clone(), 0, false)
        .map_err(|_| "could not create keyUp event")?;
    key_up.set_string_from_utf16_unchecked(utf16);
    key_up.set_integer_value_field(SYNTHETIC_MARK_FIELD as CGEventField, 1);
    key_up.post(CGEventTapLocation::HID);

    Ok(())
}
