use actix_web::{App, HttpResponse, HttpServer, Responder, delete, get, patch, post, web};
use personal_crm::{AuthUser, db};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::collections::HashMap;
use time::PrimitiveDateTime;

/// Health check endpoint for load balancers and monitoring
#[get("/health")]
async fn health_check() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "service": "personal-crm"
    }))
}

/// Verify a contact belongs to the authenticated user
async fn verify_contact_ownership(
    pool: &PgPool,
    contact_id: i32,
    user_id: i32,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "SELECT contact_id FROM contacts WHERE contact_id = $1 AND user_id = $2",
        contact_id,
        user_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(result.is_some())
}

/// Verify a tag belongs to the authenticated user
async fn verify_tag_ownership(
    pool: &PgPool,
    tag_id: i32,
    user_id: i32,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "SELECT tag_id FROM tags WHERE tag_id = $1 AND user_id = $2",
        tag_id,
        user_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(result.is_some())
}

/// Verify an interaction belongs to the authenticated user
async fn verify_interaction_ownership(
    pool: &PgPool,
    interaction_id: i32,
    user_id: i32,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "SELECT interaction_id FROM interactions WHERE interaction_id = $1 AND user_id = $2",
        interaction_id,
        user_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(result.is_some())
}

/// Verify an occasion belongs to the authenticated user
async fn verify_occasion_ownership(
    pool: &PgPool,
    occasion_id: i32,
    user_id: i32,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "SELECT occasion_id FROM occasions WHERE occasion_id = $1 AND user_id = $2",
        occasion_id,
        user_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(result.is_some())
}

#[derive(Serialize, Deserialize, Clone, FromRow)]
struct Contact {
    contact_id: i32,
    first_name: Option<String>,
    last_name: Option<String>,
    email: Option<String>,
    phone: Option<String>,
    short_note: Option<String>,
    notes: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ContactResponse {
    contact: Contact,
    tags: Vec<Tag>,
    interactions: Vec<Interaction>,
    occasions: Vec<Occasion>,
    predicted_contact_priority: Option<f32>,
}

impl ContactResponse {
    /// Calculate predicted contact priority based on interactions and occasions
    /// This is a placeholder for future implementation
    /// Currently, we calculate the average number of days between interactions
    /// and use that to estimate how soon the next interaction should be
    /// We also increase the score if an occasion is coming up
    fn new(
        contact: Contact,
        tags: Vec<Tag>,
        interactions: Vec<Interaction>,
        occasions: Vec<Occasion>,
    ) -> ContactResponse {
        let today = time::OffsetDateTime::now_utc().date();
        let days_to_closest_occasion = if !occasions.is_empty() {
            occasions
                .iter()
                .map(|occasion| {
                    let occasion_date = time::Date::from_calendar_date(
                        today.year(),
                        occasion.date.month(),
                        occasion.date.day(),
                    )
                    .unwrap();
                    let delta = occasion_date - today;
                    delta.whole_days()
                })
                .filter(|&days| days >= 0)
                .min()
        } else {
            None
        };

        let offset_from_last_interaction = if interactions.len() >= 2 {
            let mut total_days = 0;
            for i in 1..interactions.len() {
                let delta = interactions[i].interaction_date.date()
                    - interactions[i - 1].interaction_date.date();
                total_days += delta.whole_days();
            }
            let avg_days = total_days as f32 / (interactions.len() - 1) as f32;
            let last_interaction = interactions.last().unwrap();
            let delta = today - last_interaction.interaction_date.date();
            Some(delta.whole_days() as f32 - avg_days)
        } else {
            None
        };

        let predicted_contact_priority =
            match (days_to_closest_occasion, offset_from_last_interaction) {
                (Some(occ_days), Some(int_days)) => {
                    let occasion_score = if occ_days < 7 {
                        10.0
                    } else if occ_days < 30 {
                        5.0
                    } else if occ_days < 90 {
                        1.0
                    } else {
                        0.0
                    };
                    Some(int_days + occasion_score)
                }
                (Some(occ_days), None) => {
                    // Only occasion data available
                    let occasion_score = if occ_days < 7 {
                        10.0
                    } else if occ_days < 30 {
                        5.0
                    } else if occ_days < 90 {
                        1.0
                    } else {
                        0.0
                    };
                    Some(occasion_score)
                }
                (None, Some(int_days)) => {
                    // Only interaction data available
                    Some(int_days)
                }
                (None, None) => None, // No data available
            };

        ContactResponse {
            contact,
            tags,
            interactions,
            occasions,
            predicted_contact_priority,
        }
    }
}

#[derive(Deserialize, Serialize)]
struct NewContactRequest {
    first_name: Option<String>,
    last_name: Option<String>,
    email: Option<String>,
    phone: Option<String>,
    short_note: Option<String>,
    notes: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Tag {
    tag_id: i32,
    name: String,
    color: Option<String>,
    details: Option<String>,
}

#[derive(Deserialize)]
struct NewTagRequest {
    name: String,
    color: Option<String>,
    details: Option<String>,
}

#[derive(Serialize)]
struct TagResponse {
    tags: Vec<Tag>,
}

mod date_format {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use time::Date;
    use time::macros::format_description;

    const FORMAT: &[time::format_description::BorrowedFormatItem<'static>] =
        format_description!("[year]-[month]-[day]");

    pub fn serialize<S>(date: &Date, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = date.format(&FORMAT).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Date, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Date::parse(&s, &FORMAT).map_err(serde::de::Error::custom)
    }
}

mod datetime_format {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use time::PrimitiveDateTime;
    use time::macros::format_description;

    const FORMAT: &[time::format_description::BorrowedFormatItem<'static>] =
        format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]");

    pub fn serialize<S>(dt: &PrimitiveDateTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = dt.format(&FORMAT).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<PrimitiveDateTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        PrimitiveDateTime::parse(&s, &FORMAT).map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct Interaction {
    interaction_id: i32,
    contact_id: i32,
    #[serde(with = "datetime_format")]
    interaction_date: PrimitiveDateTime,
    notes: Option<String>,
    follow_up_priority: Option<i32>,
}

#[derive(Deserialize)]
struct NewInteractionRequest {
    contact_id: i32,
    #[serde(with = "datetime_format")]
    interaction_date: PrimitiveDateTime,
    notes: Option<String>,
    follow_up_priority: Option<i32>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Occasion {
    occasion_id: i32,
    contact_id: i32,
    name: String,
    #[serde(with = "date_format")]
    date: time::Date,
    recurring: Option<bool>,
    recurring_interval: Option<i32>,
    details: Option<String>,
}

#[derive(Deserialize)]
struct NewOccasionRequest {
    contact_id: i32,
    name: String,
    #[serde(with = "date_format")]
    date: time::Date,
    recurring: bool,
    recurring_interval: Option<i32>,
    details: Option<String>,
}

#[get("/contacts")]
async fn list_contacts(pool: web::Data<PgPool>, auth_user: AuthUser) -> impl Responder {
    // Get contacts for the user
    let contacts_result: Result<Vec<Contact>, _> = sqlx::query_as(
        "SELECT contact_id, first_name, last_name, email, phone, short_note, notes 
         FROM contacts 
         WHERE user_id = $1 
         ORDER BY last_name, first_name",
    )
    .bind(auth_user.user_id)
    .fetch_all(pool.get_ref())
    .await;

    let contacts = match contacts_result {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "Database error fetching contacts for user {}: {:?}",
                auth_user.user_id, e
            );
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to fetch contacts",
                "details": format!("{:?}", e)
            }));
        }
    };

    if contacts.is_empty() {
        return HttpResponse::Ok().json(Vec::<ContactResponse>::new());
    }

    let contact_ids: Vec<i32> = contacts.iter().map(|c| c.contact_id).collect();

    // Get all interactions for these contacts
    let interactions = sqlx::query_as!(
        Interaction,
        "SELECT interaction_id, contact_id, interaction_date, notes, followup_priority as follow_up_priority
         FROM interactions 
         WHERE contact_id = ANY($1)",
        &contact_ids
    )
    .fetch_all(pool.get_ref())
    .await
    .unwrap_or_default();

    // Get all occasions for these contacts
    let occasions = sqlx::query_as!(
        Occasion,
        "SELECT occasion_id, contact_id, name, date, recurring, recurring_interval, details
         FROM occasions 
         WHERE contact_id = ANY($1)",
        &contact_ids
    )
    .fetch_all(pool.get_ref())
    .await
    .unwrap_or_default();

    // Get all tags for these contacts
    let contact_tags = sqlx::query!(
        "SELECT ct.contact_id, t.tag_id, t.name, t.color, t.details
         FROM contact_tags ct
         JOIN tags t ON ct.tag_id = t.tag_id
         WHERE ct.contact_id = ANY($1)",
        &contact_ids
    )
    .fetch_all(pool.get_ref())
    .await
    .unwrap_or_default();

    // Group interactions by contact_id
    let mut interactions_map: HashMap<i32, Vec<Interaction>> = HashMap::new();
    for interaction in interactions {
        interactions_map
            .entry(interaction.contact_id)
            .or_insert_with(Vec::new)
            .push(interaction);
    }

    // Group occasions by contact_id
    let mut occasions_map: HashMap<i32, Vec<Occasion>> = HashMap::new();
    for occasion in occasions {
        occasions_map
            .entry(occasion.contact_id)
            .or_insert_with(Vec::new)
            .push(occasion);
    }

    // Group tags by contact_id
    let mut tags_map: HashMap<i32, Vec<Tag>> = HashMap::new();
    for tag in contact_tags {
        tags_map
            .entry(tag.contact_id)
            .or_insert_with(Vec::new)
            .push(Tag {
                tag_id: tag.tag_id,
                name: tag.name,
                color: tag.color,
                details: tag.details,
            });
    }

    // Build the response
    let response: Vec<ContactResponse> = contacts
        .into_iter()
        .map(|contact| {
            let contact_id = contact.contact_id;
            ContactResponse::new(
                contact,
                tags_map.remove(&contact_id).unwrap_or_default(),
                interactions_map.remove(&contact_id).unwrap_or_default(),
                occasions_map.remove(&contact_id).unwrap_or_default(),
            )
        })
        .collect();

    HttpResponse::Ok().json(response)
}

#[post("/contacts")]
async fn create_contact(
    pool: web::Data<PgPool>,
    auth_user: AuthUser,
    new_contact: web::Json<NewContactRequest>,
) -> impl Responder {
    let result = sqlx::query!(
        "INSERT INTO contacts (user_id, first_name, last_name, email, phone, short_note, notes) 
         VALUES ($1, $2, $3, $4, $5, $6, $7) 
         RETURNING contact_id",
        auth_user.user_id,
        new_contact.first_name.as_deref(),
        new_contact.last_name.as_deref(),
        new_contact.email.as_deref(),
        new_contact.phone.as_deref(),
        new_contact.short_note.as_deref(),
        new_contact.notes.as_deref(),
    )
    .fetch_one(pool.get_ref())
    .await;

    match result {
        Ok(record) => HttpResponse::Ok().json(serde_json::json!({
            "contact_id": record.contact_id,
            "message": "Contact created successfully"
        })),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            HttpResponse::InternalServerError().body("Failed to create contact")
        }
    }
}

#[post("/contacts/bulk")]
async fn create_contacts_bulk(
    pool: web::Data<PgPool>,
    auth_user: AuthUser,
    new_contacts: web::Json<Vec<NewContactRequest>>,
) -> impl Responder {
    let mut created_ids = Vec::new();
    let mut errors = Vec::new();

    for (index, contact) in new_contacts.iter().enumerate() {
        let result = sqlx::query!(
            "INSERT INTO contacts (user_id, first_name, last_name, email, phone, short_note, notes) 
             VALUES ($1, $2, $3, $4, $5, $6, $7) 
             RETURNING contact_id",
            auth_user.user_id,
            contact.first_name.as_deref(),
            contact.last_name.as_deref(),
            contact.email.as_deref(),
            contact.phone.as_deref(),
            contact.short_note.as_deref(),
            contact.notes.as_deref(),
        )
        .fetch_one(pool.get_ref())
        .await;

        match result {
            Ok(record) => created_ids.push(record.contact_id),
            Err(e) => {
                eprintln!("Database error creating contact {}: {:?}", index, e);
                errors.push(serde_json::json!({
                    "index": index,
                    "error": format!("{:?}", e)
                }));
            }
        }
    }

    HttpResponse::Ok().json(serde_json::json!({
        "created_contact_ids": created_ids,
        "errors": errors,
        "message": format!("Created {} contacts", created_ids.len())
    }))
}

#[delete("/contacts/{id}")]
async fn delete_contact(
    pool: web::Data<PgPool>,
    auth_user: AuthUser,
    contact_id: web::Path<i32>,
) -> impl Responder {
    let id = contact_id.into_inner();

    let result = sqlx::query!(
        "DELETE FROM contacts WHERE contact_id = $1 AND user_id = $2",
        id,
        auth_user.user_id,
    )
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(r) if r.rows_affected() == 0 => HttpResponse::NotFound().body("Contact not found"),
        Ok(_) => HttpResponse::Ok().body("Contact deleted successfully"),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            HttpResponse::InternalServerError().body("Failed to delete contact")
        }
    }
}

#[patch("/contacts/{id}")]
async fn update_contact(
    pool: web::Data<PgPool>,
    auth_user: AuthUser,
    contact_id: web::Path<i32>,
    updated_contact: web::Json<NewContactRequest>,
) -> impl Responder {
    let id = contact_id.into_inner();

    let result = sqlx::query!(
        "UPDATE contacts 
         SET first_name = $1, last_name = $2, email = $3, phone = $4, short_note = $5, notes = $6 
         WHERE contact_id = $7 AND user_id = $8",
        updated_contact.first_name.as_deref(),
        updated_contact.last_name.as_deref(),
        updated_contact.email.as_deref(),
        updated_contact.phone.as_deref(),
        updated_contact.short_note.as_deref(),
        updated_contact.notes.as_deref(),
        id,
        auth_user.user_id,
    )
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(r) if r.rows_affected() == 0 => HttpResponse::NotFound().body("Contact not found"),
        Ok(_) => HttpResponse::Ok().body("Contact updated successfully"),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            HttpResponse::InternalServerError().body("Failed to update contact")
        }
    }
}

#[get("/contacts/{id}")]
async fn get_contact(
    pool: web::Data<PgPool>,
    auth_user: AuthUser,
    contact_id: web::Path<i32>,
) -> impl Responder {
    let id = contact_id.into_inner();

    // Get the contact
    let contact_result: Result<Option<Contact>, _> = sqlx::query_as(
        "SELECT contact_id, first_name, last_name, email, phone, short_note, notes 
         FROM contacts 
         WHERE contact_id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(auth_user.user_id)
    .fetch_optional(pool.get_ref())
    .await;

    let contact = match contact_result {
        Ok(Some(c)) => c,
        Ok(None) => return HttpResponse::NotFound().body("Contact not found"),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            return HttpResponse::InternalServerError().body("Failed to fetch contact");
        }
    };

    // Get interactions for this contact
    let interactions = sqlx::query_as!(
        Interaction,
        "SELECT interaction_id, contact_id, interaction_date, notes, followup_priority as follow_up_priority
         FROM interactions 
         WHERE contact_id = $1",
        id
    )
    .fetch_all(pool.get_ref())
    .await
    .unwrap_or_default();

    // Get occasions for this contact
    let occasions = sqlx::query_as!(
        Occasion,
        "SELECT occasion_id, contact_id, name, date, recurring, recurring_interval, details
         FROM occasions 
         WHERE contact_id = $1",
        id
    )
    .fetch_all(pool.get_ref())
    .await
    .unwrap_or_default();

    // Get tags for this contact
    let tags = sqlx::query_as!(
        Tag,
        "SELECT t.tag_id, t.name, t.color, t.details
         FROM contact_tags ct
         JOIN tags t ON ct.tag_id = t.tag_id
         WHERE ct.contact_id = $1",
        id
    )
    .fetch_all(pool.get_ref())
    .await
    .unwrap_or_default();

    HttpResponse::Ok().json(ContactResponse::new(contact, tags, interactions, occasions))
}

#[post("/tags")]
async fn create_tag(
    pool: web::Data<PgPool>,
    auth_user: AuthUser,
    new_tag: web::Json<NewTagRequest>,
) -> impl Responder {
    let result = sqlx::query!(
        "INSERT INTO tags (user_id, name, color, details) 
         VALUES ($1, $2, $3, $4) 
         RETURNING tag_id",
        auth_user.user_id,
        new_tag.name,
        new_tag.color.as_deref(),
        new_tag.details.as_deref(),
    )
    .fetch_one(pool.get_ref())
    .await;

    match result {
        Ok(record) => HttpResponse::Ok().json(serde_json::json!({
            "tag_id": record.tag_id,
            "message": "Tag created successfully"
        })),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            HttpResponse::InternalServerError().body("Failed to create tag")
        }
    }
}

#[delete("/tags/{id}")]
async fn delete_tag(
    pool: web::Data<PgPool>,
    auth_user: AuthUser,
    tag_id: web::Path<i32>,
) -> impl Responder {
    let id = tag_id.into_inner();

    let result = sqlx::query!(
        "DELETE FROM tags WHERE tag_id = $1 AND user_id = $2",
        id,
        auth_user.user_id,
    )
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(r) if r.rows_affected() == 0 => HttpResponse::NotFound().body("Tag not found"),
        Ok(_) => HttpResponse::Ok().body("Tag deleted successfully"),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            HttpResponse::InternalServerError().body("Failed to delete tag")
        }
    }
}

#[patch("/tags/{id}")]
async fn update_tag(
    pool: web::Data<PgPool>,
    auth_user: AuthUser,
    tag_id: web::Path<i32>,
    updated_tag: web::Json<NewTagRequest>,
) -> impl Responder {
    let id = tag_id.into_inner();

    let result = sqlx::query!(
        "UPDATE tags SET name = $1, color = $2, details = $3 WHERE tag_id = $4 AND user_id = $5",
        updated_tag.name,
        updated_tag.color.as_deref(),
        updated_tag.details.as_deref(),
        id,
        auth_user.user_id,
    )
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(r) if r.rows_affected() == 0 => HttpResponse::NotFound().body("Tag not found"),
        Ok(_) => HttpResponse::Ok().body("Tag updated successfully"),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            HttpResponse::InternalServerError().body("Failed to update tag")
        }
    }
}

#[get("/tags")]
async fn list_tags(pool: web::Data<PgPool>, auth_user: AuthUser) -> impl Responder {
    let result = sqlx::query_as!(
        Tag,
        "SELECT tag_id, name, color, details FROM tags WHERE user_id = $1",
        auth_user.user_id,
    )
    .fetch_all(pool.get_ref())
    .await;

    match result {
        Ok(tags) => HttpResponse::Ok().json(TagResponse { tags }),
        Err(e) => {
            eprintln!(
                "Database error fetching tags for user {}: {:?}",
                auth_user.user_id, e
            );
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to fetch tags",
                "details": format!("{:?}", e)
            }))
        }
    }
}

#[post("/contacts/{contact_id}/tags/{tag_id}")]
async fn add_tag_to_contact(
    pool: web::Data<PgPool>,
    auth_user: AuthUser,
    path: web::Path<(i32, i32)>,
) -> impl Responder {
    let (contact_id, tag_id) = path.into_inner();

    // Verify the contact belongs to the user
    match verify_contact_ownership(pool.get_ref(), contact_id, auth_user.user_id).await {
        Ok(false) => return HttpResponse::NotFound().body("Contact not found"),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            return HttpResponse::InternalServerError().body("Database error");
        }
        Ok(true) => {}
    }

    // Verify the tag belongs to the user
    match verify_tag_ownership(pool.get_ref(), tag_id, auth_user.user_id).await {
        Ok(false) => return HttpResponse::NotFound().body("Tag not found"),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            return HttpResponse::InternalServerError().body("Database error");
        }
        Ok(true) => {}
    }

    let result = sqlx::query!(
        "INSERT INTO contact_tags (contact_id, tag_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        contact_id,
        tag_id,
    )
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({
            "message": "Tag added to contact successfully"
        })),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            HttpResponse::InternalServerError().body("Failed to add tag to contact")
        }
    }
}

#[delete("/contacts/{contact_id}/tags/{tag_id}")]
async fn remove_tag_from_contact(
    pool: web::Data<PgPool>,
    auth_user: AuthUser,
    path: web::Path<(i32, i32)>,
) -> impl Responder {
    let (contact_id, tag_id) = path.into_inner();

    // Verify the contact belongs to the user
    match verify_contact_ownership(pool.get_ref(), contact_id, auth_user.user_id).await {
        Ok(false) => return HttpResponse::NotFound().body("Contact not found"),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            return HttpResponse::InternalServerError().body("Database error");
        }
        Ok(true) => {}
    }

    let result = sqlx::query!(
        "DELETE FROM contact_tags WHERE contact_id = $1 AND tag_id = $2",
        contact_id,
        tag_id,
    )
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(_) => HttpResponse::Ok().body("Tag removed from contact successfully"),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            HttpResponse::InternalServerError().body("Failed to remove tag from contact")
        }
    }
}

#[derive(Deserialize)]
struct BulkTagAssignRequest {
    contact_ids: Vec<i32>,
}

#[post("/tags/{tag_id}/contacts/bulk")]
async fn bulk_add_tag_to_contacts(
    pool: web::Data<PgPool>,
    auth_user: AuthUser,
    tag_id: web::Path<i32>,
    request: web::Json<BulkTagAssignRequest>,
) -> impl Responder {
    let tag_id = tag_id.into_inner();

    // Verify the tag belongs to the user
    match verify_tag_ownership(pool.get_ref(), tag_id, auth_user.user_id).await {
        Ok(false) => return HttpResponse::NotFound().body("Tag not found"),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            return HttpResponse::InternalServerError().body("Database error");
        }
        Ok(true) => {}
    }

    let mut success_count = 0;
    let mut errors = Vec::new();

    for contact_id in &request.contact_ids {
        // Verify each contact belongs to the user
        match verify_contact_ownership(pool.get_ref(), *contact_id, auth_user.user_id).await {
            Ok(false) => {
                errors.push(
                    serde_json::json!({"contact_id": contact_id, "error": "Contact not found"}),
                );
                continue;
            }
            Err(e) => {
                errors.push(
                    serde_json::json!({"contact_id": contact_id, "error": format!("{:?}", e)}),
                );
                continue;
            }
            Ok(true) => {}
        }

        let result = sqlx::query!(
            "INSERT INTO contact_tags (contact_id, tag_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            contact_id,
            tag_id,
        )
        .execute(pool.get_ref())
        .await;

        match result {
            Ok(_) => success_count += 1,
            Err(e) => {
                errors.push(
                    serde_json::json!({"contact_id": contact_id, "error": format!("{:?}", e)}),
                );
            }
        }
    }

    HttpResponse::Ok().json(serde_json::json!({
        "success_count": success_count,
        "errors": errors,
        "message": format!("Added tag to {} contacts", success_count)
    }))
}

#[derive(Deserialize)]
struct BulkDeleteRequest {
    contact_ids: Vec<i32>,
}

#[post("/contacts/bulk-delete")]
async fn bulk_delete_contacts(
    pool: web::Data<PgPool>,
    auth_user: AuthUser,
    request: web::Json<BulkDeleteRequest>,
) -> impl Responder {
    let mut success_count = 0;
    let mut errors = Vec::new();

    for contact_id in &request.contact_ids {
        // Verify each contact belongs to the user
        match verify_contact_ownership(pool.get_ref(), *contact_id, auth_user.user_id).await {
            Ok(false) => {
                errors.push(
                    serde_json::json!({"contact_id": contact_id, "error": "Contact not found"}),
                );
                continue;
            }
            Err(e) => {
                errors.push(
                    serde_json::json!({"contact_id": contact_id, "error": format!("{:?}", e)}),
                );
                continue;
            }
            Ok(true) => {}
        }

        let result = sqlx::query!(
            "DELETE FROM contacts WHERE contact_id = $1 AND user_id = $2",
            contact_id,
            auth_user.user_id,
        )
        .execute(pool.get_ref())
        .await;

        match result {
            Ok(_) => success_count += 1,
            Err(e) => {
                errors.push(
                    serde_json::json!({"contact_id": contact_id, "error": format!("{:?}", e)}),
                );
            }
        }
    }

    HttpResponse::Ok().json(serde_json::json!({
        "deleted_count": success_count,
        "errors": errors,
        "message": format!("Deleted {} contacts", success_count)
    }))
}

#[post("/interactions")]
async fn create_interaction(
    pool: web::Data<PgPool>,
    auth_user: AuthUser,
    new_interaction: web::Json<NewInteractionRequest>,
) -> impl Responder {
    // Verify the contact belongs to the user
    match verify_contact_ownership(
        pool.get_ref(),
        new_interaction.contact_id,
        auth_user.user_id,
    )
    .await
    {
        Ok(false) => return HttpResponse::NotFound().body("Contact not found"),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            return HttpResponse::InternalServerError().body("Database error");
        }
        Ok(true) => {}
    }

    let result = sqlx::query!(
        "INSERT INTO interactions (user_id, contact_id, interaction_date, notes, followup_priority) 
         VALUES ($1, $2, $3, $4, $5) 
         RETURNING interaction_id",
        auth_user.user_id,
        new_interaction.contact_id,
        new_interaction.interaction_date,
        new_interaction.notes,
        new_interaction.follow_up_priority,
    )
    .fetch_one(pool.get_ref())
    .await;

    match result {
        Ok(record) => HttpResponse::Ok().json(serde_json::json!({
            "interaction_id": record.interaction_id,
            "message": "Interaction created successfully"
        })),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            HttpResponse::InternalServerError().body("Failed to create interaction")
        }
    }
}

#[delete("/interactions/{id}")]
async fn delete_interaction(
    pool: web::Data<PgPool>,
    auth_user: AuthUser,
    interaction_id: web::Path<i32>,
) -> impl Responder {
    let id = interaction_id.into_inner();

    // Verify the interaction belongs to the user
    match verify_interaction_ownership(pool.get_ref(), id, auth_user.user_id).await {
        Ok(false) => return HttpResponse::NotFound().body("Interaction not found"),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            return HttpResponse::InternalServerError().body("Database error");
        }
        Ok(true) => {}
    }

    let result = sqlx::query!(
        "DELETE FROM interactions WHERE interaction_id = $1 AND user_id = $2",
        id,
        auth_user.user_id,
    )
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(_) => HttpResponse::Ok().body("Interaction deleted successfully"),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            HttpResponse::InternalServerError().body("Failed to delete interaction")
        }
    }
}

#[patch("/interactions/{id}")]
async fn update_interaction(
    pool: web::Data<PgPool>,
    auth_user: AuthUser,
    interaction_id: web::Path<i32>,
    updated_interaction: web::Json<NewInteractionRequest>,
) -> impl Responder {
    let id = interaction_id.into_inner();

    // Verify the interaction belongs to the user
    match verify_interaction_ownership(pool.get_ref(), id, auth_user.user_id).await {
        Ok(false) => return HttpResponse::NotFound().body("Interaction not found"),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            return HttpResponse::InternalServerError().body("Database error");
        }
        Ok(true) => {}
    }

    let result = sqlx::query!(
        "UPDATE interactions SET interaction_date = $1, notes = $2, followup_priority = $3 WHERE interaction_id = $4 AND user_id = $5",
        updated_interaction.interaction_date,
        updated_interaction.notes,
        updated_interaction.follow_up_priority,
        id,
        auth_user.user_id,
    )
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(_) => HttpResponse::Ok().body("Interaction updated successfully"),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            HttpResponse::InternalServerError().body("Failed to update interaction")
        }
    }
}

#[post("/occasions")]
async fn create_occasion(
    pool: web::Data<PgPool>,
    auth_user: AuthUser,
    new_occasion: web::Json<NewOccasionRequest>,
) -> impl Responder {
    // Verify the contact belongs to the user
    match verify_contact_ownership(pool.get_ref(), new_occasion.contact_id, auth_user.user_id).await
    {
        Ok(false) => return HttpResponse::NotFound().body("Contact not found"),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            return HttpResponse::InternalServerError().body("Database error");
        }
        Ok(true) => {}
    }

    let result = sqlx::query!(
        "INSERT INTO occasions (user_id, contact_id, name, date, recurring, recurring_interval, details) 
         VALUES ($1, $2, $3, $4, $5, $6, $7) 
         RETURNING occasion_id",
        auth_user.user_id,
        new_occasion.contact_id,
        new_occasion.name,
        new_occasion.date,
        new_occasion.recurring,
        new_occasion.recurring_interval,
        new_occasion.details.as_deref(),
    )
    .fetch_one(pool.get_ref())
    .await;

    match result {
        Ok(record) => HttpResponse::Ok().json(serde_json::json!({
            "occasion_id": record.occasion_id,
            "message": "Occasion created successfully"
        })),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            HttpResponse::InternalServerError().body("Failed to create occasion")
        }
    }
}

#[delete("/occasions/{id}")]
async fn delete_occasion(
    pool: web::Data<PgPool>,
    auth_user: AuthUser,
    occasion_id: web::Path<i32>,
) -> impl Responder {
    let id = occasion_id.into_inner();

    // Verify the occasion belongs to the user
    match verify_occasion_ownership(pool.get_ref(), id, auth_user.user_id).await {
        Ok(false) => return HttpResponse::NotFound().body("Occasion not found"),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            return HttpResponse::InternalServerError().body("Database error");
        }
        Ok(true) => {}
    }

    let result = sqlx::query!(
        "DELETE FROM occasions WHERE occasion_id = $1 AND user_id = $2",
        id,
        auth_user.user_id,
    )
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(r) if r.rows_affected() == 0 => HttpResponse::NotFound().body("Occasion not found"),
        Ok(_) => HttpResponse::Ok().body("Occasion deleted successfully"),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            HttpResponse::InternalServerError().body("Failed to delete occasion")
        }
    }
}

#[patch("/occasions/{id}")]
async fn update_occasion(
    pool: web::Data<PgPool>,
    auth_user: AuthUser,
    occasion_id: web::Path<i32>,
    updated_occasion: web::Json<NewOccasionRequest>,
) -> impl Responder {
    let id = occasion_id.into_inner();

    // Verify the occasion belongs to the user
    match verify_occasion_ownership(pool.get_ref(), id, auth_user.user_id).await {
        Ok(false) => return HttpResponse::NotFound().body("Occasion not found"),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            return HttpResponse::InternalServerError().body("Database error");
        }
        Ok(true) => {}
    }

    let result = sqlx::query!(
        "UPDATE occasions SET name = $1, date = $2, recurring = $3, recurring_interval = $4, details = $5 WHERE occasion_id = $6 AND user_id = $7",
        updated_occasion.name,
        updated_occasion.date,
        updated_occasion.recurring,
        updated_occasion.recurring_interval,
        updated_occasion.details.as_deref(),
        id,
        auth_user.user_id,
    )
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(_) => HttpResponse::Ok().body("Occasion updated successfully"),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            HttpResponse::InternalServerError().body("Failed to update occasion")
        }
    }
}

/// Delete the authenticated user's account and all associated data
#[delete("/account")]
async fn delete_account(pool: web::Data<PgPool>, auth_user: AuthUser) -> impl Responder {
    match sqlx::query!("DELETE FROM users WHERE user_id = $1", auth_user.user_id)
        .execute(pool.get_ref())
        .await
    {
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(e) => {
            eprintln!("Failed to delete account: {:?}", e);
            HttpResponse::InternalServerError().body("Failed to delete account")
        }
    }
}

#[actix_web::main]
async fn main() {
    dotenvy::dotenv().ok();

    let pool = db().await;
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let bind_addr = format!("0.0.0.0:{}", port);

    println!("Starting server on {}", bind_addr);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(health_check)
            .service(list_contacts)
            .service(get_contact)
            .service(create_contact)
            .service(create_contacts_bulk)
            .service(update_contact)
            .service(delete_contact)
            .service(create_tag)
            .service(delete_tag)
            .service(update_tag)
            .service(list_tags)
            .service(add_tag_to_contact)
            .service(remove_tag_from_contact)
            .service(bulk_add_tag_to_contacts)
            .service(bulk_delete_contacts)
            .service(create_interaction)
            .service(delete_interaction)
            .service(update_interaction)
            .service(create_occasion)
            .service(delete_occasion)
            .service(update_occasion)
            .service(delete_account)
    })
    .bind(&bind_addr)
    .expect(&format!("Failed to bind to {}", bind_addr))
    .run()
    .await
    .unwrap()
}
