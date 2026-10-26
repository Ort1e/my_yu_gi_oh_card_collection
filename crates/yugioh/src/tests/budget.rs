//! Budget management.

use super::*;

#[tokio::test]
async fn manage_budgets_and_apports() {
    let t = TestApp::new().await;
    assert_eq!(
        t.post(
            "/budget/manage/budget",
            &[("month", "2025-03"), ("amount", "25")]
        )
        .await
        .0,
        StatusCode::SEE_OTHER
    );
    t.post(
        "/budget/manage/budget",
        &[("month", "2025-03"), ("amount", "30,00")],
    )
    .await;
    assert_eq!(t.scalar("SELECT count(*) FROM monthly_budget").await, 1);
    assert_eq!(
        t.text("SELECT amount FROM monthly_budget").await.as_deref(),
        Some("30.00")
    );
    assert_eq!(
        t.text("SELECT month FROM monthly_budget").await.as_deref(),
        Some("2025-03-01")
    );

    let budget_id = t.scalar("SELECT id FROM monthly_budget").await.to_string();
    let (status, _) = t
        .post(
            "/budget/manage/apports",
            &[
                ("budget_id", &budget_id),
                ("date", "2025-03-15"),
                ("amount", "10"),
                ("description", "gift"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let apport_id = t.scalar("SELECT id FROM apport").await;

    let (_, page) = t.get("/budget").await;
    assert!(page.contains("40.00"), "budget includes the apport");

    t.post(&format!("/budget/manage/apports/{apport_id}/delete"), &[])
        .await;
    assert_eq!(t.scalar("SELECT count(*) FROM apport").await, 0);

    let (status, page) = t
        .post(
            "/budget/manage/budget",
            &[("month", "march"), ("amount", "1")],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("is not a valid month"));
}
