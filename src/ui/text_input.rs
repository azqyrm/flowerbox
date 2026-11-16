//! Typing text into a `String` from the keyboard (the chat line, the name field in the settings).
//! Reads Bevy's [`KeyboardInput`] messages: their logical key carries the typed character in the
//! user's keyboard layout (umlauts, shift, dead keys …), repeats included.

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};

/// What a batch of key presses did to the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    /// Typed (or nothing happened).
    Typing,
    /// Enter: done.
    Submit,
    /// Esc: throw it away.
    Cancel,
}

/// Applies `events` to `text` (at most `max_chars` characters; `allowed` filters typed ones).
/// Stops at Enter / Esc (later keys in the same frame are dropped with it).
pub fn type_into<'a>(text: &mut String, events: impl IntoIterator<Item = &'a KeyboardInput>, max_chars: usize, allowed: impl Fn(char) -> bool) -> Edit {
    for ev in events {
        if ev.state != ButtonState::Pressed {
            continue;
        }
        match &ev.logical_key {
            Key::Enter => return Edit::Submit,
            Key::Escape => return Edit::Cancel,
            Key::Backspace => {
                text.pop();
            }
            Key::Space => push(text, ' ', max_chars, &allowed),
            Key::Character(s) => {
                for c in s.chars() {
                    push(text, c, max_chars, &allowed);
                }
            }
            _ => {}
        }
    }
    Edit::Typing
}

fn push(text: &mut String, c: char, max_chars: usize, allowed: &impl Fn(char) -> bool) {
    if !c.is_control() && allowed(c) && text.chars().count() < max_chars {
        text.push(c);
    }
}

/// Any printable character.
pub fn any_char(_: char) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::keyboard::KeyCode;
    use bevy::prelude::Entity;

    fn press(key: Key) -> KeyboardInput {
        KeyboardInput { key_code: KeyCode::KeyA, logical_key: key, state: ButtonState::Pressed, text: None, repeat: false, window: Entity::PLACEHOLDER }
    }

    #[test]
    fn typing_backspace_limit_and_submit() {
        let mut t = String::new();
        let evs = [press(Key::Character("H".into())), press(Key::Character("ä".into())), press(Key::Space), press(Key::Character("x".into())), press(Key::Backspace)];
        assert_eq!(type_into(&mut t, &evs, 10, any_char), Edit::Typing);
        assert_eq!(t, "Hä ");
        let evs = [press(Key::Character("abcdefghij".into()))];
        type_into(&mut t, &evs, 5, any_char);
        assert_eq!(t, "Hä ab", "stops at the limit");
        let evs = [press(Key::Enter), press(Key::Character("z".into()))];
        assert_eq!(type_into(&mut t, &evs, 10, any_char), Edit::Submit);
        assert_eq!(t, "Hä ab");
        assert_eq!(type_into(&mut t, &[press(Key::Escape)], 10, any_char), Edit::Cancel);
        let mut n = String::new();
        type_into(&mut n, &[press(Key::Character("a!b".into()))], 10, |c| c.is_alphanumeric());
        assert_eq!(n, "ab", "filtered");
    }
}
