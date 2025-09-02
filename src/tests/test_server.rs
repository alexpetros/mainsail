use std::ops::Deref;

use http::Uri;
use wiremock::matchers::method;
use wiremock::matchers::path;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;

use crate::activitypub::objects::actor::Actor;
use crate::database::Database;

use super::add_external_actor_at_domain;
use super::get_named_memory_db;

pub struct TestServer {
    server: MockServer,
    db: Database,
}

impl TestServer {
    pub async fn new(db_name: &str) -> TestServer {
        let server = MockServer::start().await;
        let db = get_named_memory_db(db_name);
        TestServer { server, db }
    }

    pub fn new_actor(&mut self) -> Actor {
        add_external_actor_at_domain(&mut self.db, &self.server.uri()).unwrap()
    }

    pub async fn set_inbox_response(&self, actor: &Actor, status_code: u16) {
        let uri: Uri = actor.inbox.clone().parse().unwrap();
        Mock::given(method("POST"))
            .and(path(uri.path()))
            .respond_with(ResponseTemplate::new(status_code))
            .mount(&self.server)
            .await;
    }
}

impl Deref for TestServer {
    type Target = MockServer;
    fn deref(&self) -> &Self::Target {
        &self.server
    }
}
