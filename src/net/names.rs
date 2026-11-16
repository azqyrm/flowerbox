//! Player names. Everyone picks theirs in the settings (`Settings::name`); without one you are
//! **Player1** offline, **Host** when hosting and **Guest1**, **Guest2** … as a guest.
//!
//! Online the host owns the names (like the rest of the world): a guest sends the name it wants in
//! its `Hello` (and `Rename` when it changes it), the host makes it unique and sends everyone the
//! whole list ([`Reliable::Names`]). A name already taken (case doesn't matter) gets a random German
//! noun attached: the second *gordelius* becomes *gordelius_Radieschen*.

use std::time::{SystemTime, UNIX_EPOCH};

use super::NetSession;
use super::protocol::{NetId, PLAYER_ID_BASE, Reliable, encode};
use super::transport::{NetCmd, NetLink};
use crate::tuning::MAX_NAME_CHARS;

/// Nouns for telling twins apart (ASCII spellings, so every font has the letters).
pub const NOUNS: &[&str] = &[
    "Radieschen", "Kartoffel", "Brezel", "Schnitzel", "Gurke", "Maulwurf", "Igel", "Dackel", "Wurst", "Zwiebel",
    "Kohlrabi", "Spargel", "Strudel", "Hamster", "Pilz", "Kobold", "Zwerg", "Kerze", "Laterne", "Glocke",
    "Schaufel", "Besen", "Kessel", "Pfanne", "Socke", "Stiefel", "Mantel", "Rabe", "Dachs", "Fuchs",
    "Krake", "Schnecke", "Ameise", "Hummel", "Kuchen", "Honig", "Senf", "Rettich", "Bohne", "Erbse",
    "Nudel", "Knopf", "Nagel", "Hammer", "Amboss", "Ritter", "Pilger", "Gaukler", "Narr", "Laute",
    "Harfe", "Trommel", "Fiedel", "Wanderer", "Sonnenblume", "Kompost", "Regenwurm", "Kranich", "Eule", "Kauz",
    "Lindwurm", "Knappe", "Truhe", "Fackel", "Kelch", "Krone", "Mondschein", "Nebel", "Gewitter", "Bergkristall",
];

/// Letters, digits, space, `_`, `-`, `.`.
pub fn name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, ' ' | '_' | '-' | '.')
}

/// The name as it may be used: allowed characters only, single spaces, trimmed, at most
/// `MAX_NAME_CHARS`. Empty = none chosen.
pub fn sanitize(raw: &str) -> String {
    let kept: String = raw.chars().filter(|c| name_char(*c) || c.is_whitespace()).collect();
    let words: Vec<&str> = kept.split_whitespace().collect();
    words.join(" ").chars().take(MAX_NAME_CHARS).collect::<String>().trim().to_string()
}

/// The name of a player who didn't choose one: Host (slot 0 online), GuestN, or Player1 offline.
pub fn default_name(id: NetId, online: bool) -> String {
    match id.saturating_sub(PLAYER_ID_BASE) {
        _ if !online => "Player1".into(),
        0 => "Host".into(),
        n => format!("Guest{n}"),
    }
}

/// `wanted`, or — if `taken` already says yes to it — `wanted_<Noun>` with a noun picked from
/// `seed` (another one while that's taken too).
pub fn unique_name(wanted: &str, taken: impl Fn(&str) -> bool, seed: u64) -> String {
    if !taken(wanted) {
        return wanted.to_string();
    }
    let start = (seed % NOUNS.len() as u64) as usize;
    for i in 0..NOUNS.len() {
        let candidate = format!("{wanted}_{}", NOUNS[(start + i * 7) % NOUNS.len()]);
        if !taken(&candidate) {
            return candidate;
        }
    }
    (2..).map(|n| format!("{wanted}_{n}")).find(|c| !taken(c)).expect("some number is free")
}

fn seed() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(7, |d| d.as_nanos() as u64 ^ (d.as_secs() << 17))
}

impl NetSession {
    /// A player's name as everybody sees it.
    pub fn name_of(&self, id: NetId) -> String {
        self.names.get(&id).cloned().unwrap_or_else(|| default_name(id, self.online()))
    }

    /// Host: gives player `id` the name it `wants` (made unique against everyone else's).
    pub fn assign_name(&mut self, id: NetId, wants: &str) -> String {
        let wanted = match sanitize(wants) {
            w if w.is_empty() => default_name(id, true),
            w => w,
        };
        let others: Vec<String> = self.names.iter().filter(|(i, _)| **i != id).map(|(_, n)| n.to_lowercase()).collect();
        let name = unique_name(&wanted, |n| others.contains(&n.to_lowercase()), seed().wrapping_add(id as u64));
        self.names.insert(id, name.clone());
        name
    }

    /// Host: tells every guest the names.
    pub fn share_names(&self, link: &NetLink) {
        let mut list: Vec<(NetId, String)> = self.names.iter().map(|(i, n)| (*i, n.clone())).collect();
        list.sort();
        link.send(NetCmd::Reliable { to: None, bytes: encode(&Reliable::Names(list)) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_cleaned_and_defaulted() {
        assert_eq!(sanitize("  gor  delius!\n"), "gor delius");
        assert_eq!(sanitize("Jürgen_der-2."), "Jürgen_der-2.");
        assert_eq!(sanitize("abcdefghijklmnopqrstuvwxyz").chars().count(), MAX_NAME_CHARS);
        assert_eq!(sanitize("  "), "");
        assert_eq!(default_name(PLAYER_ID_BASE, true), "Host");
        assert_eq!(default_name(PLAYER_ID_BASE + 2, true), "Guest2");
        assert_eq!(default_name(0, false), "Player1");
    }

    #[test]
    fn twins_get_a_german_noun() {
        let taken = ["gordelius".to_string()];
        let is_taken = |n: &str| taken.iter().any(|t| t.eq_ignore_ascii_case(n));
        assert_eq!(unique_name("ada", is_taken, 3), "ada");
        let twin = unique_name("Gordelius", is_taken, 3);
        let noun = twin.strip_prefix("Gordelius_").expect("name_Noun");
        assert!(NOUNS.contains(&noun) && noun.chars().next().unwrap().is_uppercase(), "{twin}");
    }

    #[test]
    fn the_host_keeps_names_unique() {
        let mut s = NetSession { role: super::super::Role::Hosting, ..Default::default() };
        assert_eq!(s.assign_name(PLAYER_ID_BASE, ""), "Host");
        assert_eq!(s.assign_name(PLAYER_ID_BASE + 1, "gordelius"), "gordelius");
        let second = s.assign_name(PLAYER_ID_BASE + 2, "GORDELIUS");
        assert!(second.starts_with("GORDELIUS_"), "{second}");
        let third = s.assign_name(PLAYER_ID_BASE + 3, "gordelius");
        assert!(third.starts_with("gordelius_") && third.to_lowercase() != second.to_lowercase(), "{third} vs {second}");
        // Renaming to your own name keeps it.
        assert_eq!(s.assign_name(PLAYER_ID_BASE + 1, "gordelius"), "gordelius");
        assert_eq!(s.assign_name(PLAYER_ID_BASE + 4, "Host").split('_').next(), Some("Host"));
        assert_ne!(s.name_of(PLAYER_ID_BASE + 4), "Host");
    }
}
