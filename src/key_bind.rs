// SPDX-License-Identifier: MPL-2.0

//! Keyboard shortcuts, as in other COSMIC apps: each one runs a menu action.

use std::collections::HashMap;

use cosmic::iced::keyboard::key::{Named, Physical};
use cosmic::iced::keyboard::{Key, Modifiers};
use cosmic::widget::menu::key_bind::{KeyBind, Modifier};

/// Directions the arrow keys move the view or the search results in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrow {
    Left,
    Right,
    Up,
    Down,
}

/// What a shortcut, or the same entry in a menu, does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuAction {
    About,
    ZoomIn,
    ZoomOut,
    /// Show the user's position.
    Locate,
    Pan(Arrow),
}

/// One shortcut per action, the one menus show.
pub fn key_binds() -> HashMap<KeyBind, MenuAction> {
    HashMap::from([
        bind(
            &[Modifier::Ctrl],
            Key::Character("+".into()),
            MenuAction::ZoomIn,
        ),
        bind(
            &[Modifier::Ctrl],
            Key::Character("-".into()),
            MenuAction::ZoomOut,
        ),
        bind(
            &[Modifier::Ctrl],
            Key::Character("l".into()),
            MenuAction::Locate,
        ),
        bind(
            &[],
            Key::Named(Named::ArrowLeft),
            MenuAction::Pan(Arrow::Left),
        ),
        bind(
            &[],
            Key::Named(Named::ArrowRight),
            MenuAction::Pan(Arrow::Right),
        ),
        bind(&[], Key::Named(Named::ArrowUp), MenuAction::Pan(Arrow::Up)),
        bind(
            &[],
            Key::Named(Named::ArrowDown),
            MenuAction::Pan(Arrow::Down),
        ),
    ])
}

/// Other keys for the same actions, kept out of menus so they show one each.
pub fn aliases() -> Vec<(KeyBind, MenuAction)> {
    vec![
        // `+` is Shift+= on many layouts.
        bind(
            &[Modifier::Ctrl, Modifier::Shift],
            Key::Character("+".into()),
            MenuAction::ZoomIn,
        ),
        bind(
            &[Modifier::Ctrl],
            Key::Character("=".into()),
            MenuAction::ZoomIn,
        ),
        // Without Ctrl, as in GNOME Maps.
        bind(&[], Key::Character("+".into()), MenuAction::ZoomIn),
        bind(
            &[Modifier::Shift],
            Key::Character("+".into()),
            MenuAction::ZoomIn,
        ),
        bind(&[], Key::Character("=".into()), MenuAction::ZoomIn),
        bind(&[], Key::Character("-".into()), MenuAction::ZoomOut),
    ]
}

/// The action of the shortcut a key press matches, if any.
pub fn action(
    key_binds: &HashMap<KeyBind, MenuAction>,
    aliases: &[(KeyBind, MenuAction)],
    (modifiers, key, physical_key): (Modifiers, &Key, Option<&Physical>),
) -> Option<MenuAction> {
    key_binds
        .iter()
        .chain(aliases.iter().map(|(key_bind, action)| (key_bind, action)))
        .find(|(key_bind, _)| key_bind.matches(modifiers, key, physical_key))
        .map(|(_, action)| *action)
}

fn bind(modifiers: &[Modifier], key: Key, action: MenuAction) -> (KeyBind, MenuAction) {
    let key_bind = KeyBind {
        modifiers: modifiers.to_vec(),
        key,
    };
    (key_bind, action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pressed(modifiers: Modifiers, key: &str) -> Option<MenuAction> {
        let key = Key::Character(key.into());
        action(&key_binds(), &aliases(), (modifiers, &key, None))
    }

    #[test]
    fn zoom_keys_with_and_without_ctrl() {
        let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(pressed(Modifiers::CTRL, "+"), Some(MenuAction::ZoomIn));
        assert_eq!(pressed(ctrl_shift, "+"), Some(MenuAction::ZoomIn));
        assert_eq!(pressed(Modifiers::CTRL, "="), Some(MenuAction::ZoomIn));
        assert_eq!(pressed(Modifiers::empty(), "+"), Some(MenuAction::ZoomIn));
        assert_eq!(pressed(Modifiers::empty(), "-"), Some(MenuAction::ZoomOut));
        assert_eq!(pressed(Modifiers::CTRL, "-"), Some(MenuAction::ZoomOut));
        assert_eq!(pressed(Modifiers::CTRL, "L"), Some(MenuAction::Locate));
        assert_eq!(pressed(Modifiers::empty(), "l"), None);
    }

    #[test]
    fn menus_show_one_shortcut_per_action() {
        let actions: Vec<_> = key_binds().into_values().collect();
        for (index, action) in actions.iter().enumerate() {
            assert!(
                !actions[..index].contains(action),
                "{action:?} has two shortcuts"
            );
        }
    }
}
