use data::new_test_actor;
use openssl::pkey::{PKey, Private};
use data::{EXT_ACTOR_1, EXT_NOTE_1, PRIVATE_KEY_STR};
use crate::activitypub::objects::actor::Actor;
use crate::local::actors::{LocalActorStub, create_local_actor_with_uuid};
use crate::local::notes;
use crate::local::InternalResult;
use crate::database::Database;
use crate::external::save_external_actor;
use crate::external::save_external_note_no_validation;

pub mod data;
pub mod test_server;
pub mod requests;

pub fn get_private_key() -> PKey<Private> {
    PKey::private_key_from_pem(PRIVATE_KEY_STR.as_bytes()).unwrap()
}

pub fn add_external_actor_at_domain(db: &mut Database, domain: &str) -> InternalResult<Actor> {
    let actor_id = format!("{domain}/actors/1");
    create_and_save_external_actor(db, &actor_id, "Test Actor", "@test")
}

fn create_and_save_external_actor(db: &mut Database, id: &str, name: &str, handle: &str) -> InternalResult<Actor> {
    let actor = new_test_actor(id, name, handle);
    save_external_actor(db, &actor)?;
    Ok(actor)
}

pub fn add_test_data(db: &mut Database) {
    let actor_domain = "example.com";
    db.set_domain(actor_domain).unwrap();

    let actor_id = create_local_actor_with_uuid(db, "1", "awp", "Alex Petros", "Main").unwrap();
    let pkey = PKey::generate_x25519().unwrap();
    let sender = LocalActorStub { id: actor_id, domain: actor_domain.to_string(), pkey };

    notes::create_with_uuid(db, "1".to_string(), &sender, "First post!", None).unwrap();
    notes::create_with_uuid(db, "2".to_string(), &sender, "Second post!", None).unwrap();
    notes::create_with_uuid(db, "3".to_string(), &sender, "Third post!", None).unwrap();

    save_external_actor(db, &EXT_ACTOR_1).unwrap();
    save_external_note_no_validation(db, &EXT_NOTE_1, &EXT_ACTOR_1.id).unwrap();
}

pub fn get_memory_db_no_test_data() -> (Database, String) {
    let db = Database::new_in_memory().unwrap();
    db.init().unwrap();
    let name = db.get_path().to_owned();
    (db, name)
}

pub fn get_named_memory_db(name: &str) -> Database {
    Database::new(name).unwrap()
}

pub fn get_memory_db() -> (Database, String) {
    let (mut db, name) = get_memory_db_no_test_data();
    add_test_data(&mut db);
    (db, name)
}
