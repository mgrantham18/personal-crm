mod common;

use common::*;

/// Test creating a contact and verifying it exists in the database
#[tokio::test]
async fn test_create_contact() {
    let test_ctx = setup_test_db().await;
    let user_id = setup_test_user(&test_ctx.pool).await;

    // Create a contact
    let result = sqlx::query!(
        "INSERT INTO contacts (user_id, first_name, last_name, email, phone, short_note, notes) 
         VALUES ($1, $2, $3, $4, $5, $6, $7) 
         RETURNING contact_id",
        user_id,
        "John",
        "Doe",
        "john.doe@example.com",
        "555-1234",
        "Met at conference",
        "Interested in collaboration"
    )
    .fetch_one(&test_ctx.pool)
    .await
    .expect("Failed to create contact");

    assert!(result.contact_id > 0);

    // Verify the contact exists
    let contact = sqlx::query!(
        "SELECT first_name, last_name, email FROM contacts WHERE contact_id = $1",
        result.contact_id
    )
    .fetch_one(&test_ctx.pool)
    .await
    .expect("Failed to fetch contact");

    assert_eq!(contact.first_name, "John");
    assert_eq!(contact.last_name, "Doe");
    assert_eq!(contact.email, "john.doe@example.com");
}

/// Test updating a contact
#[tokio::test]
async fn test_update_contact() {
    let test_ctx = setup_test_db().await;
    let user_id = setup_test_user(&test_ctx.pool).await;

    // Create a contact first
    let contact_id = sqlx::query!(
        "INSERT INTO contacts (user_id, first_name, last_name, email) 
         VALUES ($1, $2, $3, $4) RETURNING contact_id",
        user_id,
        "Jane",
        "Smith",
        "jane@example.com"
    )
    .fetch_one(&test_ctx.pool)
    .await
    .expect("Failed to create contact")
    .contact_id;

    // Update the contact
    sqlx::query!(
        "UPDATE contacts 
         SET first_name = $1, last_name = $2, email = $3, phone = $4 
         WHERE contact_id = $5 AND user_id = $6",
        "Jane",
        "Doe-Smith",
        "jane.doe@example.com",
        "555-5678",
        contact_id,
        user_id,
    )
    .execute(&test_ctx.pool)
    .await
    .expect("Failed to update contact");

    // Verify the update
    let result = sqlx::query!(
        "SELECT last_name, phone FROM contacts WHERE contact_id = $1",
        contact_id
    )
    .fetch_one(&test_ctx.pool)
    .await
    .expect("Failed to fetch updated contact");

    assert_eq!(result.last_name, "Doe-Smith");
    assert_eq!(result.phone, Some("555-5678".to_string()));
}

/// Test deleting a contact
#[tokio::test]
async fn test_delete_contact() {
    let test_ctx = setup_test_db().await;
    let user_id = setup_test_user(&test_ctx.pool).await;

    // Create a contact first
    let contact_id = sqlx::query!(
        "INSERT INTO contacts (user_id, first_name, last_name, email) 
         VALUES ($1, $2, $3, $4) RETURNING contact_id",
        user_id,
        "Bob",
        "Johnson",
        "bob@example.com"
    )
    .fetch_one(&test_ctx.pool)
    .await
    .expect("Failed to create contact")
    .contact_id;

    // Delete the contact
    sqlx::query!(
        "DELETE FROM contacts WHERE contact_id = $1 AND user_id = $2",
        contact_id,
        user_id,
    )
    .execute(&test_ctx.pool)
    .await
    .expect("Failed to delete contact");

    // Verify deletion
    let result = sqlx::query!(
        "SELECT contact_id FROM contacts WHERE contact_id = $1",
        contact_id
    )
    .fetch_optional(&test_ctx.pool)
    .await
    .expect("Failed to check contact deletion");

    assert!(result.is_none());
}

/// Test listing contacts for a user
#[tokio::test]
async fn test_list_contacts() {
    let test_ctx = setup_test_db().await;
    let user_id = setup_test_user(&test_ctx.pool).await;

    // Create multiple contacts
    for i in 1..=3 {
        sqlx::query!(
            "INSERT INTO contacts (user_id, first_name, last_name, email) 
             VALUES ($1, $2, $3, $4)",
            user_id,
            format!("User{}", i),
            format!("Test{}", i),
            format!("user{}@example.com", i)
        )
        .execute(&test_ctx.pool)
        .await
        .expect("Failed to create contact");
    }

    // List contacts for this user
    let contacts = sqlx::query!(
        "SELECT contact_id, first_name, last_name 
         FROM contacts 
         WHERE user_id = $1 
         ORDER BY last_name",
        user_id
    )
    .fetch_all(&test_ctx.pool)
    .await
    .expect("Failed to list contacts");

    assert_eq!(contacts.len(), 3);
    assert_eq!(contacts[0].first_name, "User1");
    assert_eq!(contacts[1].first_name, "User2");
    assert_eq!(contacts[2].first_name, "User3");
}
