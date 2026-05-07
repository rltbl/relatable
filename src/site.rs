use crate as rltbl;
use rltbl::user::{Account, UserCursor};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Site {
    pub title: String,
    pub root: String,
    pub editable: bool,
    pub user: Account,
    pub users: IndexMap<String, UserCursor>,
    pub tables: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Page {
    pub path: String,
    pub formats: IndexMap<String, String>,
    pub tabs: Vec<Tab>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tab {
    pub table: String,
    pub active: bool,
    pub url: String,
    pub count: String,
}
