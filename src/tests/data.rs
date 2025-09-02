use std::sync::LazyLock;
use crate::activitypub::objects::actor::{Actor, ActorType, PublicKey};
use crate::activitypub::BASIC_CONTEXT;
use crate::activitypub::objects::note::{Note, NoteType};
use crate::activitypub::signature::generate_pkey;

pub static EXT_ACTOR_1: LazyLock<Actor> = LazyLock::new(|| {
    new_test_actor("https://example.com/users/test_actor_1", "Test Actor", "@test")
});

pub static EXT_ACTOR_2: LazyLock<Actor> = LazyLock::new(|| {
    new_test_actor("https://example.com/users/test_actor_2", "Test Actor 2", "@test2")
});

pub static EXT_NOTE_1: LazyLock<Note> = LazyLock::new(|| {
    new_test_note("https://example.com/statuses/1", &EXT_ACTOR_1.id, "Test content")
});

pub fn new_test_actor(id: &str, name: &str, handle: &str) -> Actor {
    let pkey = generate_pkey().unwrap();
    let public_key_pem = pkey.public_key_to_pem().unwrap();
    let public_key = PublicKey {
        id: format!("{id}#main-key"),
        owner: id.to_string(),
        public_key_pem: String::from_utf8(public_key_pem).unwrap(),
    };

    Actor {
        context: BASIC_CONTEXT,
        id: id.to_string(),
        name: Some(name.to_string()),
        actor_type: ActorType::Person,
        url: id.to_string(),
        preferred_username: format!("@{handle}"),
        inbox: format!("{id}/inbox"),
        outbox: format!("{id}/outbox"),
        followers: Some(format!("{id}/followers")),
        following: Some(format!("{id}/following")),
        summary: Some("".to_string()),
        public_key,
        icon: None,
    }
}

pub fn new_test_note(url: &str, actor_id: &str, content: &str) -> Note {
    Note {
        id: url.to_string(),
        _type: NoteType::Note,
        url: url.to_string(),
        summary: None,
        published: None,
        in_reply_to: None,
        attributed_to: actor_id.to_string(),
        to: vec![],
        cc: vec![],
        sensitive: false,
        content: content.to_string(),
        attachment: vec![],
        tag: vec![],
    }
}

pub const PRIVATE_KEY_STR: &str = "
-----BEGIN PRIVATE KEY-----
MIIEvwIBADANBgkqhkiG9w0BAQEFAASCBKkwggSlAgEAAoIBAQDbq3Z0Q5hI09Fg
OFRnhSxVl5lKnUjDNhGkTyl0MheuepHtbp7IPx2M6c8neJoNiU+RT2eJZqRF9i2u
DWWgQ47dOPgW2+EVc/wWe2aZ2KnoVP0Zdlx8no9i6AgPem1FKAcomjetpMyY0KTY
qQgqLCdTL6efxms8Q2aAswZaHWSu6BKXQAXKXEa3zNRqTKazSzcmOZzn9JiKQQ97
26LC3/62H+CuO9OfbH27VHvFCljIm6HS6YPt5X6IvYPsRpB1mWs2L5DbUcHX95Sy
nuUsdVuil/ZqcrX/6vsYLVQAlrvtYka7GGzMELwwzjlt8/rQPLrUaviDOUpS6osc
T2uh7Yw5AgMBAAECggEAHuX+Um+TmGLa/OrPdy7fQ+bVXrkYRq/F9CkCZAq2aV9q
Q/h0XdGq6WrrLYUkPiqF8Pg3UGIJ/KFFCZ9uyl0rrOhaKnT2iHBOZtHSu78jHxVH
80XtJRufKu1c/OMJQwSqBRxeyo5Mt7iusfRiEUnb+jJ7edjiCpGnIGE4z9d56wli
K8wsCcwCMMKeMoTr+VhWaLKT+z92TGp8YzIMEzjIpswugw6e5dHY7mMO9dGaTKpf
oUaYLrJA3KivdV09ALD9C6uZVPll+BcSzHjndHLrw+vppN0aUcTtMqZjBwjfYH6j
0XDKcBxUrIxk1KZ2Z+gsMGSExbuc0J+0e5A4AReLZQKBgQD9KUugC7WruH/4jbBK
rRf3UGph6EZOIDk9vz3CEwZv0Mxpdvw9wgj6VD+iAEH4H2HzY1CZjOGrSl58JNd9
gpEZqDG+0BC2SCpmGr8n1UN4lChPqJMP9rVamvb/ZLG4M14y/R/DhPt/uGa1RQbh
A6mYOvp3fuokGXeYXb8VZG9jXwKBgQDeIgd4o5Kxtf2jqyEXBmeKDAJUCyWnFqsz
NjG4vGDcXCvDcJwcCnL/bfgIkzb+k5naP9gLU1SThX7TGxZ5o2U/Lg2tcdjCIa4X
UyGRS6/1zldH3vKHEN4MU072fc/4rqOCcXuowxOVz+rXxGDwtOhCU10M9LW4IBbS
CEHU6/YPZwKBgQDGOjORGhoiX9E0Wh2UG30Mldb22t1SB0FK7Bf7gtIjc6bbXeOc
p/ZHrPg2bWU6V7hmz3kpAnshmJXrNx3UlQvwvxZUhq7PTCqG0KKlGgQ+wiPeW+zr
Lw5J59VHnRzbUr2Lx0+71MDo/QbPIUGFp0yAeo3sS2Ai7B4NHDqnEYF2mQKBgQCA
E4jRQycCthcfdj0nBHdQfiodgSvdklF1vqqehGcoxzAoiNZJyLCTnDll4FPWJw6P
az5LEWnzgQYl/DlSOybWbdxPBBRhon+5TSJOnyA+jxF+DAs0W0JcQvCFaPcvwKng
PUR6grfZKj+S1tS6FOTR0TUWkkXwevgNc9tZRA/kwwKBgQDEnM3YyHJpbPr7DSt6
M7pBzBXSB2tq/KSJygpWyjwDz3YGrLtRkRGf70zHYRLEC34ltLchmJmIkHKIoxRB
d6RwVoXndGSsIwxJL9Ab506aRnHiN/jZPCqifgaIdOr9uVZFjNuDPMn/WOwHdKsd
3Lp7bQySBUIoNrgDvbI3uR0fjA==
-----END PRIVATE KEY-----
";
