use rosu_v2::prelude::*;
use serde::{Deserialize, Serialize};
use serenity::all::{async_trait, Client, Colour, CommandInteraction, Context, CreateEmbed, CreateEmbedFooter, CreateInteractionResponse, CreateInteractionResponseMessage, EventHandler, GatewayIntents, Interaction};
use serenity::prelude::Mentionable;
use serenity::model::application::Command;
use serenity::prelude::TypeMapKey;
use serenity_commands::Commands;
use sqlx::{postgres::PgPool, Pool, Postgres, Row};
use std::env;
use dotenv::dotenv;

struct PostgresPool;
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
                    .content(format!("Generando ranking para {}...", cmd.user.id.mention()))
                    .embed(embed)
            },
            AllCommands::AddUser { osu_username: osu_id } => {
                let data_read = ctx.data.read().await;
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
                    // Check if the user already exists
                    let query = format!("SELECT * FROM osu_{} WHERE osu_id = $1", guild_id_str);
                    let exists = sqlx::query(&query)
                        .bind(&osu_id)
                        .fetch_optional(&pool)
                        .await
                        .expect("Error consultando usuario existente")
                        .is_some();
                    if exists {
                        CreateInteractionResponseMessage::new()
                            .content(format!("El usuario con id de osu {} ya está registrado en la base de datos del servidor osu_{}", osu_id, guild_id_str))
                    } else {
                        match osu.user(osu_id.clone()).await {
                            Ok(_) => {
                                sqlx::query(format!("INSERT INTO osu_{} (osu_id) VALUES ($1)",guild_id_str).as_ref())
                                    .bind(&osu_id)
                                    .execute(&pool)
                                    .await
                                    .expect("Error insertando usuario");
                                CreateInteractionResponseMessage::new()
                                    .content(format!("Usuario con id de osu {} se ha registrado en la base de datos de este servidor correctamente", osu_id))
                            }
                            Err(_) => {
                                CreateInteractionResponseMessage::new()
                                    .content(format!("El usuario {} no existe en osu", osu_id))
                            }
                        }
                    }

            },
        }
    }
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, _: serenity::all::Ready) {
        let database_url = env::var("DATABASE_URL").expect("Missing DATABASE_URL env");
        let pool = PgPool::connect(&database_url)
            .await
            .expect("Error connecting to database");
        {
            let mut data = ctx.data.write().await;
            data.insert::<PostgresPool>(pool);
        }

        Command::set_global_commands(
            &ctx.http,
            AllCommands::create_commands(),
        )
        .await
        .expect("Error registrando comandos globales");
    }

    // Escuchamos interacciones (slash commands)
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(cmd) = interaction {
            let osu_client_id: u64 = env::var("OSU_CLIENT_ID")
                .expect("Missing OSU_CLIENT_ID")
                .parse()
                .expect("OSU_CLIENT_ID must be a valid number");
            let osu_client_secret: String = env::var("OSU_CLIENT_SECRET")
                .expect("Missing OSU_CLIENT_SECRET");
            let osu = Osu::new(osu_client_id, osu_client_secret)
                .await
                .expect("Failed to create Osu client");

            // Extraemos nuestro enum desde los datos
            let data = AllCommands::from_command_data(&cmd.data)
                .expect("Error parseando comando");
            let response = data.run(&ctx, &cmd, osu).await;
            cmd.create_response(
                &ctx.http,
                CreateInteractionResponse::ChannelMessageWithSource(response),
            )
            .await
            .expect("Error creando respuesta");
        }
    }
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    let token = env::var("DISCORD_TOKEN").expect("Missing DISCORD_TOKEN");

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
