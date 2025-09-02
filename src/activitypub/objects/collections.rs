//! ActivityPub [Collections](https://www.w3.org/TR/activitystreams-core/#collection)
use serde::{Deserialize, Serialize};
use crate::activitypub::objects::activities::create::CreateActivity;
use crate::activitypub::{ApUrl, AtContext, Context};

#[derive(Debug, Serialize, Deserialize)]
pub enum OrderedCollectionType {
    OrderedCollection,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PageOrLink {
    Link(String),
    Page(OrderedCollectionPage),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OrderedCollectionPage {
    #[serde(rename = "@context")]
    pub context: AtContext,
    pub next: Option<Box<PageOrLink>>,
    pub prev: Option<Box<PageOrLink>>,
    #[serde(rename = "orderedItems")]
    pub ordered_items: Vec<CreateActivity>,
}
pub type OutboxPage = OrderedCollectionPage;


#[derive(Debug, Serialize, Deserialize)]
pub struct OrderedCollection<T: Serialize> {
    #[serde(rename = "@context")]
    pub context: AtContext,
    #[serde(rename = "type")]
    _type: OrderedCollectionType,
    pub id: String,
    #[serde(rename = "totalItems")]
    pub total_items: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first: Option<PageOrLink>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last: Option<PageOrLink>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<PageOrLink>, // This is theoretically mandatory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<T>>,
}

impl<T: Serialize> OrderedCollection<T> {
    pub fn new(id: ApUrl, first_page: ApUrl, last_page: ApUrl, total_items: u64) -> OrderedCollection<T> {
        let first_page = PageOrLink::Link(first_page);
        let last_page = PageOrLink::Link(last_page);
        OrderedCollection {
            context: AtContext::Context(Context::ActivityStreams),
            id,
            _type: OrderedCollectionType::OrderedCollection,
            total_items,
            items: None,
            first: Some(first_page),
            last: Some(last_page),
            current: None,
        }
    }

    pub fn new_from_list(id: ApUrl, items: Vec<T>) -> OrderedCollection<T> {
        let total_items = items.len() as u64;
        OrderedCollection {
            context: AtContext::Context(Context::ActivityStreams),
            id,
            _type: OrderedCollectionType::OrderedCollection,
            total_items,
            items: Some(items),
            first: None,
            last: None,
            current: None,
        }
    }
}

