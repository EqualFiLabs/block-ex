use clap::Parser;

#[derive(Parser, Debug, Clone)]
pub struct Config {
    #[arg(long, env = "API_BIND", default_value = "0.0.0.0:8081")]
    pub bind: String,
    #[arg(long, env = "DATABASE_URL")]
    pub database_url: String,
    #[arg(long, env = "REDIS_URL", default_value = "redis://127.0.0.1:6379")]
    pub redis_url: String,
    #[arg(long, env = "NETWORK", default_value = "stagenet")]
    pub network: String,
    #[arg(long, env = "FINALITY_WINDOW", default_value_t = 30)]
    pub finality_window: u32,
}
