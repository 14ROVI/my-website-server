mod lastfm;
mod letterboxd;
mod notes;

use actix_web::{
    web::{self, Data},
    App, HttpServer,
};
use dotenvy::dotenv;
use sqlx::{Pool, Sqlite, SqlitePool};
use std::env;

type DbPool = Data<Pool<Sqlite>>;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().expect("Couldn't load .env");

    let pool = SqlitePool::connect(&env::var("DATABASE_URL").expect("Can't find DATABASE_URL"))
        .await
        .unwrap();

    return HttpServer::new(move || {
        App::new()
            .app_data(Data::new(pool.clone()))
            .service(web::scope("/notes").configure(notes::config))
            .service(web::scope("/films").configure(letterboxd::config))
            .service(web::scope("/lastfm").configure(lastfm::config))
    })
    .bind(("127.0.0.1", 2323))?
    .run()
    .await;
}
