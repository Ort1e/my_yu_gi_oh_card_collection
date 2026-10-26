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

#[tokio::test]
async fn a_refund_lowers_the_cost_of_its_lot() {
    let t = TestApp::new().await;
    t.post(
        "/budget/manage/budget",
        &[("month", "2025-06"), ("amount", "50")],
    )
    .await;
    // 10.50 paid, 2 cards at 2.25: 6.00 of shipping, 2.50 of it refunded
    let mut lot = LOT.to_vec();
    lot.push(("refund", "2,50"));
    assert_eq!(t.post("/lots/add", &lot).await.0, StatusCode::SEE_OTHER);
    assert_eq!(
        t.text("SELECT price || ' ' || refund FROM lot")
            .await
            .as_deref(),
        Some("10.50 2.50")
    );

    let (_, page) = t.get("/lots/1").await;
    assert!(page.contains("10.50 € (refunded 2.50 €)"), "{page}");

    // spent 8.00 in June, of which 3.50 of shipping
    let (_, budget) = t.get("/budget").await;
    assert!(
        budget.contains("<td>8.00</td>") && budget.contains("<td>3.50</td>"),
        "{budget}"
    );

    // the edit page keeps it; more than the price is refused
    let (_, edit) = t.get("/lots/1/edit").await;
    assert!(
        edit.contains(r#"name="refund" id="refund" value="2.50""#),
        "{edit}"
    );
    let (status, page) = t
        .post(
            "/lots/1/edit",
            &[
                ("lot_type", "purchase"),
                ("price", "10.50"),
                ("buy_date", "2025-06-12"),
                ("no_card_price", "0"),
                ("refund", "11"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("The refund must be between 0 and the price."));
}
