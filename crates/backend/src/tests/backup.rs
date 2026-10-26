//! `backup`: a consistent copy of the database and the media, while the app
//! runs.

use super::*;

#[tokio::test]
async fn the_backup_copies_the_database_and_mirrors_the_media() {
    let t = TestApp::new().await;
    t.post(
        "/decks/add",
        &[("name", "Kept deck"), ("version_name", "v1")],
    )
    .await;
    let photos = t.state.media_dir.join("advent_calendar_photos/2025");
    std::fs::create_dir_all(&photos).unwrap();
    std::fs::write(photos.join("03.jpg"), "photo").unwrap();
    std::fs::write(t.state.media_dir.join("half.tmp"), "downloading").unwrap();

    let to = t.state.media_dir.parent().unwrap().join("backups");
    let report = crate::commands::backup::backup(&t.state, &to, 14)
        .await
        .unwrap();
    assert_eq!(report.media_copied, 1, "the .tmp file is skipped");
    assert!(to.join("media/advent_calendar_photos/2025/03.jpg").exists());

    // the copy is a database with the data
    let copy = sqlx::SqlitePool::connect(&format!("sqlite:{}?mode=ro", report.database.display()))
        .await
        .unwrap();
    let decks: i64 = sqlx::query_scalar("SELECT count(*) FROM deck WHERE name = 'Kept deck'")
        .fetch_one(&copy)
        .await
        .unwrap();
    assert_eq!(decks, 1);
    copy.close().await;

    // again the same day: replaced, nothing new to copy
    let again = crate::commands::backup::backup(&t.state, &to, 14)
        .await
        .unwrap();
    assert_eq!(again.database, report.database);
    assert_eq!(again.media_copied, 0);
}
