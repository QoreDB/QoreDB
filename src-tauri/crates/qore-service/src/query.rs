// SPDX-License-Identifier: Apache-2.0

use qore_core::{Namespace, SessionId, TableSchema};
use qore_drivers::session_manager::SessionManager;

use crate::error::ServiceError;
use crate::virtual_relations::VirtualRelationStore;

pub async fn describe_table(
    session_manager: &SessionManager,
    vr_store: &VirtualRelationStore,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    connection_id: Option<&str>,
) -> Result<TableSchema, ServiceError> {
    let driver = session_manager.get_driver(session).await?;
    let mut schema = driver.describe_table(session, namespace, table).await?;

    if let Some(conn_id) = connection_id {
        let virtual_fks = vr_store.get_foreign_keys_for_table(
            conn_id,
            &namespace.database,
            namespace.schema.as_deref(),
            table,
        );
        for vfk in virtual_fks {
            let is_duplicate = schema.foreign_keys.iter().any(|fk| {
                fk.column == vfk.column
                    && fk.referenced_table == vfk.referenced_table
                    && fk.referenced_column == vfk.referenced_column
            });
            if !is_duplicate {
                schema.foreign_keys.push(vfk);
            }
        }
    }

    Ok(schema)
}
