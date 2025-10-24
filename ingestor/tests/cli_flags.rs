use clap::{Parser, Subcommand};
use serial_test::serial;
use std::env;
use std::ffi::OsString;

#[test]
#[serial]
fn parse_defaults() {
    env::remove_var("INGEST_CONCURRENCY");
    env::remove_var("RPC_RPS");
    env::remove_var("BOOTSTRAP");
    env::remove_var("CONCURRENCY");
    let mut v = vec![OsString::from("ingestor"), OsString::from("run")];
    v.push("--database-url".into());
    v.push("postgres://x:x@localhost/x".into());
    let args = super_args(v);
    assert_eq!(args.ingest_concurrency, 8);
    assert_eq!(args.rpc_rps, 10);
    assert!(!args.bootstrap);
}

#[test]
#[serial]
fn parse_env_overrides() {
    env::set_var("INGEST_CONCURRENCY", "32");
    env::set_var("RPC_RPS", "99");
    env::set_var("BOOTSTRAP", "true");
    env::remove_var("CONCURRENCY");
    let args = super_args(vec![
        OsString::from("ingestor"),
        OsString::from("run"),
        OsString::from("--database-url"),
        OsString::from("postgres://x:x@localhost/x"),
    ]);
    assert_eq!(args.ingest_concurrency, 32);
    assert_eq!(args.rpc_rps, 99);
    assert!(args.bootstrap);
    env::remove_var("INGEST_CONCURRENCY");
    env::remove_var("RPC_RPS");
    env::remove_var("BOOTSTRAP");
    env::remove_var("CONCURRENCY");
}

fn super_args<I>(itr: I) -> RunArgs
where
    I: IntoIterator<Item = OsString>,
{
    let cli = TestCli::parse_from(itr);
    match cli.command {
        TestCmd::Run(args) => args,
    }
}

// Bring in Args
use ingestor::cli::RunArgs;

#[derive(Parser)]
struct TestCli {
    #[command(subcommand)]
    command: TestCmd,
}

#[derive(Subcommand)]
enum TestCmd {
    Run(RunArgs),
}
