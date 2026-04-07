mod lastfm;
mod letterboxd;
mod notes;

use actix_cors::Cors;
use actix_web::{
    http,
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
        let cors = Cors::default()
            .allowed_origin("https://rovi.me")
            .allowed_methods(vec!["GET", "POST", "OPTIONS"])
            .allowed_headers(vec![
                http::header::AUTHORIZATION,
                http::header::ACCEPT,
                http::header::CONTENT_TYPE,
            ])
            .max_age(3600);

        App::new()
            .wrap(cors)
            .app_data(Data::new(pool.clone()))
            .service(web::scope("/notes").configure(notes::config))
            .service(web::scope("/films").configure(letterboxd::config))
            .service(web::scope("/lastfm").configure(lastfm::config))
    })
    .bind(("127.0.0.1", 2323))?
    .run()
    .await;
}
