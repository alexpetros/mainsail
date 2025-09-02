use crate::server::ap_request::UnverifiedApRequest;
use crate::local::notes::get_note;
use crate::server::response;
use crate::server::response::ServerResult;

pub async fn get(req: UnverifiedApRequest) -> ServerResult {
    let note_id = req.get_url_param(2, "Missing note ID")?;
    let note = get_note(&req.db, note_id)?;

    match note {
        None => response::not_found(),
        Some(note) => response::send(note.into_note().json())
    }
}

#[cfg(test)]
mod tests {
    use http::StatusCode;

    use crate::activitypub::objects::note::Note;
    use crate::tests::requests::create_unverified_get;
    use crate::tests::get_memory_db;

    #[tokio::test]
    async fn get_note() {
        let (_db, name) = get_memory_db();
        let uri = "/notes/1";
        let req = create_unverified_get(uri, &name);
        let res = super::get(req).await.unwrap();

        let body = res.into_body_string().await;
        let note: Note = serde_json::from_str(&body).unwrap();

        assert_eq!(note.id, "https://example.com/notes/1");
    }

    #[tokio::test]
    async fn get_note_not_found() {
        let (_db, name) = get_memory_db();
        let uri = "/notes/199";
        let req = create_unverified_get(uri, &name);
        let res = super::get(req).await.unwrap();

        assert_eq!(res.status(), StatusCode::NOT_FOUND)
    }
}

