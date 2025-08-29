// this is meant to add a user to the database
use bcrypt::hash;
use dotenv::dotenv;
use sqlx::postgres::PgPoolOptions;
use std::{
    env,
    io::{self, Write},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db = PgPoolOptions::new().connect(&db_url).await?;

    let bcrypt_cost: u32 = env::var("BCRYPT_COST")
        .unwrap_or(String::from("10"))
        .parse()
        .unwrap_or(10);

    let mut username = String::new();
    get_response("Enter username", &mut username)?;

    let mut password = String::new();
    get_response("Enter password", &mut password)?;

    let view_history = loop {
        print!("View history? [Y/n]: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        match input.to_lowercase().trim() {
            "y" | "yes" | "" => break true,
            "n" | "no" => break false,
            _ => {
                println!("Invalid input");
                continue;
            }
        }
    };

    let password_hash = hash(password.trim(), bcrypt_cost)?;
    println!("{}", password_hash);
    sqlx::query!(
        "INSERT INTO users (username, password_hash, view_history) VALUES ($1, $2, $3)",
        username,
        password_hash,
        view_history
    )
    .execute(&db)
    .await?;

    Ok(())
}

fn get_response(question: &str, output: &mut String) -> Result<(), Box<dyn std::error::Error>> {
    print!("{}: ", question);
    io::stdout().flush()?;

    io::stdin().read_line(output)?;

    Ok(())
}
