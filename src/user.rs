use crate as rltbl;
use rltbl::core::RowID;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Account {
    pub name: String,
    pub color: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[allow(dead_code)]
pub struct Cursor {
    pub table: String,
    pub row: RowID,
    pub column: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct UserCursor {
    pub name: String,
    pub color: String,
    pub cursor: Cursor,
    pub datetime: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use serde_json;

    // Test inner JSON string.
    #[tokio::test]
    async fn test_user() -> Result<()> {
        let user_cursor = UserCursor {
            name: "john".to_owned(),
            color: "#000000".to_owned(),
            cursor: Cursor {
                table: "foo".to_owned(),
                row: 1,
                column: "bar".to_owned(),
            },
            datetime: "2025-01-01T00:00:00".to_owned(),
        };
        let string = r##"{"name":"john","color":"#000000","cursor":{"table":"foo","row":1,"column":"bar"},"datetime":"2025-01-01T00:00:00"}"##;
        assert_eq!(serde_json::from_str::<UserCursor>(&string)?, user_cursor,);
        assert_eq!(serde_json::to_string(&user_cursor)?, string,);
        Ok(())
    }
}
