//! The Yu-Gi-Oh! maintenance commands (the backend parses the command line
//! and runs them): Konami's ban lists, the card clean-up, the Yu-Gi-Oh! part
//! of the price refresh, the tournaments' old notes.

pub mod ban_list;
pub mod clean_cards;
pub mod prices;
pub mod tournament_notes;
