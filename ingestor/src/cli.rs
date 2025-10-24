use clap::Args as ClapArgs;

#[derive(ClapArgs, Debug)]
pub struct RunArgs {
    #[arg(long, env = "DATABASE_URL")]
    pub database_url: String,
    #[arg(
        long,
        env = "XMR_RPC_URL",
        default_value = "http://127.0.0.1:38081/json_rpc"
    )]
    pub rpc_url: String,
    #[arg(long, env = "FINALITY_WINDOW", default_value_t = 30)]
    pub finality_window: u64,
    #[arg(
        long = "ingest-concurrency",
        env = "INGEST_CONCURRENCY",
        default_value_t = 8,
        alias = "concurrency"
    )]
    pub ingest_concurrency: usize,
    #[arg(
        long = "rpc-requests-per-second",
        env = "RPC_RPS",
        default_value_t = 10
    )]
    pub rpc_rps: u32,
    #[arg(
        long,
        env = "BOOTSTRAP",
        default_value_t = false,
        help = "Bootstrap mode relaxes limits & disables analytics, for fastest initial sync"
    )]
    pub bootstrap: bool,
    #[arg(long, env = "START_HEIGHT")]
    pub start_height: Option<u64>,
    #[arg(long, env = "LIMIT", help = "Optional limit of blocks to sync")]
    pub limit: Option<u64>,
    #[arg(
        long,
        env = "XMR_ZMQ_URL",
        default_value = "tcp://127.0.0.1:38082",
        help = "Monero ZMQ publisher providing raw_tx/raw_block topics"
    )]
    pub zmq_url: String,
}
