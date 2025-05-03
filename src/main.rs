use postgresql_embedded::{PostgreSQL, Status, BOOTSTRAP_DATABASE};
use rosu_v2::prelude::*;
use serde::{Deserialize, Serialize};
use serenity::all::{async_trait, Client, Colour, CommandInteraction, Context, CreateEmbed, CreateEmbedFooter, CreateInteractionResponse, CreateInteractionResponseMessage, EventHandler, GatewayIntents, Interaction, UserId};
use serenity::model::application::Command;
use serenity::prelude::TypeMapKey;
use serenity_commands::Commands;
use sqlx::{postgres::PgPool, Pool, Postgres, Row};

struct PostgresKey;
struct PostgresPool;
impl TypeMapKey for PostgresKey {
    type Value = PostgreSQL;
}

impl TypeMapKey for PostgresPool {
    type Value = Pool<Postgres>;
}

#[derive(Debug, Serialize, Deserialize, PartialEq, PartialOrd, Clone)]
struct PlayerData {
    osu_id: String,
    pp: f32,
}

// Define sólo el comando Ping
#[derive(Debug, Commands)]
enum AllCommands {
    /// Genera el ranking actual de los jugadores de osu del servidor
    Rank,
    /// Añade un osuario de osu al ranking
    AddUser {
        /// osu username
        osu_username: String,
    },
}

impl AllCommands {
    async fn run(self, ctx: &Context, cmd: &CommandInteraction, osu: Osu) -> CreateInteractionResponseMessage {
        match self {
            AllCommands::Rank => {
                let data_read = ctx.data.read().await;
                let pool = data_read
                    .get::<PostgresPool>()
                    .expect("No se encontró el pool de Postgres en el TypeMap")
                    .clone();

                // Get the guild ID as a string
                let guild_id_str = cmd.guild_id.map_or(
                    "Ningún servidor (DM)".to_string(),
                    |gid| gid.to_string(),
                );

                // Create the table query
                let create_table_query = format!(
                    "CREATE TABLE IF NOT EXISTS osu_{} (
                    id           SERIAL PRIMARY KEY,
                    osu_id TEXT   NOT NULL
                    )",
                    guild_id_str
                );

                // Create the table if it doesn't exist
                let _result = sqlx::query(&create_table_query)
                    .execute(&pool)
                    .await
                    .expect("Error creating table");

                // Query to get all users
                let query = format!("SELECT * FROM osu_{}", guild_id_str);

                // Execute the query
                let rows = match sqlx::query(&query).fetch_all(&pool).await {
                    Ok(rows) => rows,
                    Err(e) => {
                        return CreateInteractionResponseMessage::new()
                            .content(format!("Error al obtener la lista de usuarios: {}", e));
                    }
                };

                // Create the embed
                let mut embed = CreateEmbed::new()
                    .title("Leaderboard de osu!".to_string())
                    .description("Ranking de jugadores registrados")
                    .color(Colour::PURPLE)
                    .footer(CreateEmbedFooter::new("Powered by osu!"))
                    .thumbnail("https://upload.wikimedia.org/wikipedia/commons/thumb/1/1e/Osu%21_Logo_2016.svg/1200px-Osu%21_Logo_2016.svg.png");

                let mut osu_data:Vec<PlayerData> = vec![];

                // Add users to the embed
                if rows.is_empty() {
                    embed = embed.field("Sin usuarios", "No hay jugadores registrados en este servidor.", false);
                 } else {
                    for (_index, row) in rows.iter().enumerate() {
                        let osu_id: &str = row.try_get("osu_id").unwrap();
                        osu_data.push(PlayerData{osu_id: osu_id.to_owned(), pp: osu.user(osu_id).await.unwrap().statistics.unwrap().pp});
                    }

                    osu_data.sort_by(|a, b| b.pp.partial_cmp(&a.pp).unwrap());

                    let mut counter = 1;
                    for local_player in osu_data.iter().take(15) {
                        embed = embed.field(
                            format!("#{} {}",counter,local_player.osu_id),
                            format!("pp: {:?}", osu.user(local_player.osu_id.clone()).await.unwrap().statistics.unwrap().pp),
                            false
                        );
                        counter+=1;
                    }
                }
                CreateInteractionResponseMessage::new()
                    .embed(embed)
            },
            AllCommands::AddUser { osu_username: osu_id } => {
                let data_read = ctx.data.read().await;
                if cmd.user.id==UserId::new(976878661242331156){
                    let guild_id_str = cmd.guild_id.map_or(
                        "Ningún servidor (DM)".to_string(),
                        |gid| gid.to_string(),
                    );


                    let pool = data_read
                        .get::<PostgresPool>()
                        .expect("No se encontró el pool de Postgres en el TypeMap")
                        .clone();
                    sqlx::query(format!(
                        "CREATE TABLE IF NOT EXISTS osu_{} (
                        id           SERIAL PRIMARY KEY,
                        osu_id TEXT   NOT NULL
                        )",
                        guild_id_str).as_ref())
                        .execute(&pool)
                        .await
                        .expect("Error creando tabla");
                    sqlx::query(format!("INSERT INTO osu_{} (osu_id) VALUES ($1)",guild_id_str).as_ref())
                        .bind(&osu_id)
                        .execute(&pool)
                        .await
                        .expect("Error insertando usuario");
                    CreateInteractionResponseMessage::new()
                        .content(format!("Usuario con id de osu {} en la base de datos del servidor osu_{} se ha registrado en la base de datos correctamente", osu_id, guild_id_str))
                } else {
                    CreateInteractionResponseMessage::new()
                        .content("SOS")
                }
            },
        }
    }
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, _: serenity::all::Ready) {
        let mut postgresql = PostgreSQL::default();
        postgresql.setup().await.unwrap();
        postgresql.start().await.unwrap();
        let mut data = ctx.data.write().await;
        match postgresql.status() {
            Status::Started => {
                let settings = postgresql.settings();
                let url = settings.url(BOOTSTRAP_DATABASE);
                let pool = PgPool::connect(&url).await.unwrap();

                data.insert::<PostgresPool>(pool);
            }
            other => {
                eprintln!("⚠️ Estado inesperado de PostgreSQL: {:?}", other);
            }
        }

        data.insert::<PostgresKey>(postgresql);
        Command::set_global_commands(
            &ctx.http,
            AllCommands::create_commands(), // Vec<CreateCommand>
        )
            .await
            .expect("Error registrando comandos globales");
    }

    // Escuchamos interacciones (slash commands)
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(cmd) = interaction {

            let client_id: u64 = 40451;
            let client_secret = String::from("OXNVv1nDMM2StzYn23sN5OByxifCxnwcvasPpwCB");
            let osu = Osu::new(client_id, client_secret).await.unwrap();

            // Extraemos nuestro enum desde los datos
            let data = AllCommands::from_command_data(&cmd.data)
                .expect("Error parseando comando");
            let response = data.run(&ctx, &cmd, osu).await;
            cmd.create_response(
                &ctx.http,
                CreateInteractionResponse::Message(response),
            )
                .await
                .expect("Error creando respuesta");
        }
    }
}

#[tokio::main]
async fn main() {
    // Tu token aquí
    let token = "MTM2Nzg5MzQ2NzA4MjEyOTYzOQ.GM9m9s.oMBJw8r036mbZiSK-Te29bFtGCzvQyHcMyejR0";

    // Creamos el handler
    let handler = Handler;

    // Construimos el cliente sin GuildId
    let mut client = Client::builder(token, GatewayIntents::non_privileged())
        .event_handler(handler)
        .await
        .expect("Error creando el cliente");

    // Iniciamos el bot
    client.start().await.expect("Error iniciando el cliente");
}
