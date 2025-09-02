use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Tag {
    Mention(Mention),
    Unknown(serde_json::Value),
}


#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum MentionType {
    Mention
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Mention {
    #[serde(rename = "type")]
    _type: MentionType,
    href: String
}

impl Mention {
    pub fn new(id: String) -> Self {
        Mention {
            _type: MentionType::Mention,
            href: id
        }
    }
}
