use serde::{Deserialize, Serialize};
use url::Url;
use crate::activitypub::{AtContext, Context};
use crate::activitypub::objects::actor::{Actor, ActorType};
use crate::server::ap_request::AuthedApRequest;
use crate::local::{actors, actors::LocalActorStub, InternalResult};
use crate::server::response::{self, ServerResult};
use crate::database::Database;
use crate::external;
use crate::gateway::federation;
use crate::utils::new_activity_id;
use super::Activity;

#[derive(Debug, Serialize, Deserialize)]
enum UpdateActivityType { Update }

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateActivity<T> {
    #[serde(rename = "type")]
    activity_type: UpdateActivityType,
    #[serde(rename = "@context")]
    pub context: Option<AtContext>,
    pub id: Url,
    pub actor: String,
    pub object: T,
}

impl<T> UpdateActivity<T> {
    fn new(domain: &str, actor_id: String, object: T) -> UpdateActivity<T> {
        UpdateActivity {
            activity_type: UpdateActivityType::Update,
            context: Some(AtContext::Context(Context::ActivityStreams)),
            id: new_activity_id(domain).unwrap(),
            actor: actor_id,
            object,
        }
    }
}

impl<T: Serialize> Activity for UpdateActivity<T> {
    fn get_id(&self) -> &Url {
        &self.id
    }

    fn json(&self) -> String {
        serde_json::json!(self).to_string()
    }
}

/// Federate an update for a Person actor to that person's followers
pub fn send_person_update(db: &Database, profile: LocalActorStub) -> InternalResult<()> {
    let actor = actors::get_actor_by_id(db, &profile.id)?;
    let update = UpdateActivity::new(&profile.domain, actor.id.to_owned(), actor);
    federation::federate_activity(db, profile, update)?;
    Ok(())
}

pub fn receive(req: AuthedApRequest, activity: UpdateActivity<Actor>) -> ServerResult {
    if activity.object.actor_type != ActorType::Person {
        return response::not_implemented();
    }

    let mut db = req.db;
    let actor = activity.object;
    external::save_external_actor(&mut db, &actor)?;
    response::ok()
}

#[cfg(test)]
mod tests {
    use http::StatusCode;


    use crate::activitypub::Context;
    use crate::activitypub::AtContext;
    use crate::external::get_external_actor_from_cache;
    use crate::activitypub::objects::activities::Activity;
    use crate::activitypub::objects::actor::Actor;
    use crate::tests::data::EXT_ACTOR_1;
    use crate::tests::get_memory_db;
    use crate::tests::requests::create_authed_inbox_post;
    use crate::database::Database;
    use crate::utils::new_activity_id;
    use super::{UpdateActivity, UpdateActivityType};

    #[test]
    fn update_person_existing() {
        const NEW_NAME: &str = "New Name";
        const EXPECTED_NUM_NOTES: i64 = 1;

        let (db, name) = get_memory_db();

        let actor_id = &EXT_ACTOR_1.id;
        let old_actor = get_external_actor_from_cache(&db, actor_id).unwrap().unwrap();
        assert_eq!(old_actor.name, EXT_ACTOR_1.name);
        assert_eq!(num_external_notes(&db, actor_id), EXPECTED_NUM_NOTES);


        // Create actor from DB actor and change their name
        let mut new_actor = EXT_ACTOR_1.clone();
        new_actor.name = Some(NEW_NAME.to_string());

        let update = new_person_update(new_actor);
        let req = create_authed_inbox_post(update.json(), &name, &EXT_ACTOR_1);

        // Test that the update returned 200
        let res = super::receive(req, update).unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // Test that the update changed the name but didn't delete existing notes
        let actor = get_external_actor_from_cache(&db, actor_id).unwrap().unwrap();
        assert_eq!(actor.name.unwrap(), NEW_NAME);
        assert_eq!(num_external_notes(&db, actor_id), EXPECTED_NUM_NOTES);
    }

    pub fn new_person_update(person: Actor) -> UpdateActivity<Actor> {
        UpdateActivity {
            activity_type: UpdateActivityType::Update,
            context: Some(AtContext::Context(Context::ActivityStreams)),
            id: new_activity_id("example.com").unwrap(),
            actor: person.id.clone(),
            object: person,
        }
    }

    fn num_external_notes(db: &Database, actor_id: &str) -> i64 {
        db.query_row(
            "SELECT count(id) FROM cache WHERE actor = ?1 AND type = 'Note'",
            (actor_id,),
            |r| { r.get(0) },
        ).unwrap()
    }
}
