//! Data shared by the routes and the commands: the SQLite pool and `Money`,
//! row types, card-row writes and shared SQL, ygoprodeck card data, the
//! shipment PDF files and uploaded photos.

pub mod book_data;
pub mod books;
pub mod calendar_photos;
pub mod card_data;
pub mod cards;
pub mod db;
pub mod models;
pub mod photos;
pub mod prices;
pub mod shipment_files;
pub mod valuation;
