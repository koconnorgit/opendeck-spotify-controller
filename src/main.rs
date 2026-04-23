use openaction::OpenActionResult;

mod gfx;
mod plugin;
mod scroll;
mod spotify;
mod tiles;

#[tokio::main]
async fn main() -> OpenActionResult<()> {
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        simplelog::Config::default(),
        simplelog::TerminalMode::Stdout,
        simplelog::ColorChoice::Never,
    )
    .unwrap();

    println!("Starting Spotify Controller plugin...");

    plugin::init().await
}
