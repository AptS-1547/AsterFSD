use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ClientWhitelist::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ClientWhitelist::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(ClientWhitelist::ClientId)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(ClientWhitelist::ClientName)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ClientWhitelist::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(ClientWhitelist::CreatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // Insert default whitelisted clients
        let insert = Query::insert()
            .into_table(ClientWhitelist::Table)
            .columns([
                ClientWhitelist::ClientId,
                ClientWhitelist::ClientName,
                ClientWhitelist::Enabled,
            ])
            .values_panic(["69d7".into(), "EuroScope 3.2".into(), true.into()])
            .values_panic(["88e4".into(), "vPilot".into(), true.into()])
            .values_panic(["48e2".into(), "Swift".into(), true.into()])
            .values_panic(["de1e".into(), "VRC".into(), true.into()])
            .to_owned();

        manager.exec_stmt(insert).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ClientWhitelist::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum ClientWhitelist {
    Table,
    Id,
    ClientId,
    ClientName,
    Enabled,
    CreatedAt,
}
