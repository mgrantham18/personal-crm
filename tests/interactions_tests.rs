mod common;

use common::*;
use time::macros::datetime;

/// Test creating an interaction and verifying it exists in the database
#[tokio::test]
async fn test_create_interaction() {
    let test_ctx = setup_test_db().await;
    let user_id = setup_test_user(&test_ctx.pool).await;

    // Create a contact first
    let contact_id = sqlx::query!(
        "INSERT INTO contacts (user_id, first_name, last_name, email) 
         VALUES ($1, $2, $3, $4) RETURNING contact_id",
        user_id,
        "Alice",
        "Wonder",
        "alice@example.com"
    )
    .fetch_one(&test_ctx.pool)
    .await
    .expect("Failed to create contact")
    .contact_id;

    let interaction_date = datetime!(2026-01-04 14:30:00);

    // Create an interaction
    let result = sqlx::query!(
        "INSERT INTO interactions (contact_id, interaction_date, notes, followup_priority) 
         VALUES ($1, $2, $3, $4) 
         RETURNING interaction_id",
        contact_id,
        interaction_date,
        "Had coffee meeting",
        3
    )
    .fetch_one(&test_ctx.pool)
    .await
    .expect("Failed to create interaction");

    assert!(result.interaction_id > 0);

    // Verify in database
    let interaction = sqlx::query!(
        "SELECT notes, followup_priority FROM interactions WHERE interaction_id = $1",
        result.interaction_id
    )
    .fetch_one(&test_ctx.pool)
    .await
    .expect("Failed to fetch interaction");

    assert_eq!(interaction.notes, Some("Had coffee meeting".to_string()));
    assert_eq!(interaction.followup_priority, Some(3));
}

/// Test updating an interaction
#[tokio::test]
async fn test_update_interaction() {
    let test_ctx = setup_test_db().await;
    let user_id = setup_test_user(&test_ctx.pool).await;

    // Create a contact
    let contact_id = sqlx::query!(
        "INSERT INTO contacts (user_id, first_name, last_name, email) 
         VALUES ($1, $2, $3, $4) RETURNING contact_id",
        user_id,
        "Bob",
        "Builder",
        "bob@example.com"
    )
    .fetch_one(&test_ctx.pool)
    .await
    .expect("Failed to create contact")
    .contact_id;

    let interaction_date = datetime!(2026-01-01 10:00:00);

    // Create an interaction
    let interaction_id = sqlx::query!(
        "INSERT INTO interactions (contact_id, interaction_date, notes, followup_priority) 
         VALUES ($1, $2, $3, $4) RETURNING interaction_id",
        contact_id,
        interaction_date,
        "Initial meeting",
        1
    )
    .fetch_one(&test_ctx.pool)
    .await
    .expect("Failed to create interaction")
    .interaction_id;

    // Update the interaction
    let new_date = datetime!(2026-01-02 10:00:00);
    sqlx::query!(
        "UPDATE interactions SET interaction_date = $1, notes = $2, followup_priority = $3 WHERE interaction_id = $4",
        new_date,
        "Follow-up meeting - discussed project",
        5,
        interaction_id,
    )
    .execute(&test_ctx.pool)
    .await
    .expect("Failed to update interaction");

    // Verify the update
    let result = sqlx::query!(
        "SELECT notes, followup_priority FROM interactions WHERE interaction_id = $1",
        interaction_id
    )
    .fetch_one(&test_ctx.pool)
    .await
    .expect("Failed to fetch updated interaction");

    assert_eq!(
        result.notes,
        Some("Follow-up meeting - discussed project".to_string())
    );
    assert_eq!(result.followup_priority, Some(5));
}

/// Test deleting an interaction
#[tokio::test]
async fn test_delete_interaction() {
    let test_ctx = setup_test_db().await;
    let user_id = setup_test_user(&test_ctx.pool).await;

    // Create a contact
    let contact_id = sqlx::query!(
        "INSERT INTO contacts (user_id, first_name, last_name, email) 
         VALUES ($1, $2, $3, $4) RETURNING contact_id",
        user_id,
        "Charlie",
        "Brown",
        "charlie@example.com"
    )
    .fetch_one(&test_ctx.pool)
    .await
    .expect("Failed to create contact")
    .contact_id;

    let interaction_date = datetime!(2026-01-03 15:00:00);

    // Create an interaction
    let interaction_id = sqlx::query!(
        "INSERT INTO interactions (contact_id, interaction_date, notes) 
         VALUES ($1, $2, $3) RETURNING interaction_id",
        contact_id,
        interaction_date,
        "Phone call"
    )
    .fetch_one(&test_ctx.pool)
    .await
    .expect("Failed to create interaction")
    .interaction_id;

    // Delete the interaction
    sqlx::query!(
        "DELETE FROM interactions WHERE interaction_id = $1",
        interaction_id,
    )
    .execute(&test_ctx.pool)
    .await
    .expect("Failed to delete interaction");

    // Verify deletion
    let result = sqlx::query!(
        "SELECT interaction_id FROM interactions WHERE interaction_id = $1",
        interaction_id
    )
    .fetch_optional(&test_ctx.pool)
    .await
    .expect("Failed to check interaction deletion");

    assert!(result.is_none());
}
