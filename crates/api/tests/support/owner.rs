//! Statements run as the database's owner: what the API would refuse to
//! write, or what an operator, or an administrator through another
//! process, changes meanwhile.

use uuid::Uuid;

use super::owner_url;

/// Runs `statement`, bound to `binds`, as the database's owner.
pub async fn owner_runs(statement: &str, binds: &[Uuid]) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let mut query = sqlx::query(statement);
    for bind in binds {
        query = query.bind(*bind);
    }
    query.execute(&owner).await.unwrap();
    owner.close().await;
}
