use crate as rltbl;
use rltbl::{column::Columns, core::RowID, tsv_table::TsvTable};
use rltbl_db::any::AnyPool;

/// Represents the special "table" table.
pub struct DataTable<'a> {
    pub name: String,
    pub id: RowID,
    pub pool: &'a AnyPool,
    pub columns: Columns,
}

impl<'a> TsvTable for DataTable<'a> {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn id(&self) -> String {
        format!("t{}", self.id)
    }

    fn pool(&self) -> &AnyPool {
        self.pool
    }

    fn columns(&self) -> Columns {
        self.columns.clone()
    }
}
